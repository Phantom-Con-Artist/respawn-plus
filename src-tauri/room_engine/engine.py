#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Vessel room engine. Host operations and the confined supervisor share no shell protocol.

Packages contain data, registry recipes and DLL overrides, never host-side install scripts.
The supervisor executes all room applications in one namespace tree. The host-side client
only streams output/exit status; it cannot be asked to execute commands by the guest.
"""
import argparse
import base64
import ctypes
import ctypes.util
import errno
import hashlib
import json
import mmap
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import signal
import socket
import stat
import struct
import subprocess
import sys
import tempfile
import threading
import time

MAX_REQUEST = 1024 * 1024
SCHEMA = 1


def fail(message):
    raise ValueError(message)


def relative(value):
    if not isinstance(value, str) or not value or '\\' in value or '\x00' in value:
        fail('Package paths must be nonempty relative POSIX paths')
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in ('..', '.') for part in value.split('/')):
        fail('Package path escapes its root')
    return path


def regular(root, value):
    path = root
    for part in relative(value).parts:
        path = path / part
        if path.is_symlink():
            fail('Runtime packages may not contain symlinks')
    if not path.is_file() or not stat.S_ISREG(path.stat().st_mode):
        fail(f'Runtime payload is not a regular file: {value}')
    return path


def sha(path):
    h = hashlib.sha256()
    with open(path, 'rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def canonical_json(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=True).encode()


def checked_manifest(source):
    manifest_file = regular(source, 'manifest.json')
    if manifest_file.stat().st_size > MAX_REQUEST: fail('Runtime manifest exceeds the size limit')
    manifest = json.loads(manifest_file.read_bytes())
    if manifest.get('schema') != SCHEMA:
        fail('Unsupported Windows runtime package schema')
    for key in ('id', 'version', 'architecture'):
        if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._+-]{0,127}', manifest.get(key, '')):
            fail(f'Invalid runtime {key}')
    if manifest['architecture'] not in ('x86', 'x64', 'arm64', 'any'):
        fail('Unsupported runtime architecture')
    if not isinstance(manifest.get('files'), list) or not 0 < len(manifest['files']) <= 16384:
        fail('Runtime package has no files')
    destinations = set()
    for item in manifest['files']:
        file = regular(source, item['source'])
        target = relative(item['target'])
        if target.parts[0].lower() != 'drive_c' or len(target.parts) < 3:
            fail('Runtime files must target a path underneath drive_c')
        if any(':' in part for part in target.parts):
            fail('Runtime target contains a Windows drive or stream separator')
        key = str(target).casefold()
        if key in destinations:
            fail('Duplicate runtime destination')
        destinations.add(key)
        if not re.fullmatch(r'[a-f0-9]{64}', item.get('sha256', '')) or sha(file) != item['sha256']:
            fail(f'Runtime checksum mismatch: {item["source"]}')
    for name in manifest.get('registry', []):
        regular(source, name)
    for name, order in manifest.get('dll_overrides', {}).items():
        if not re.fullmatch(r'[A-Za-z0-9_.-]+', name) or order not in ('native', 'builtin', 'native,builtin', 'builtin,native', 'disabled'):
            fail('Invalid Wine DLL override')
    for key in ('provides', 'requires'):
        if not isinstance(manifest.get(key, []), list) or not all(isinstance(x, str) and 0 < len(x) < 256 for x in manifest.get(key, [])):
            fail(f'Invalid package {key}')
    detect = manifest.get('detect', {})
    if not isinstance(detect, dict) or any(not isinstance(detect.get(key, []),list) or not all(isinstance(item,str) and len(item)<256 for item in detect.get(key, [])) for key in ('imports','frameworks')):
        fail('Invalid runtime detection rules')
    return manifest


def package_digest(source, manifest):
    h = hashlib.sha256(canonical_json(manifest))
    # Registry data is part of the identity too, including its contents, not just a filename.
    for name in sorted(manifest.get('registry', [])):
        h.update(name.encode()); h.update(regular(source, name).read_bytes())
    return h.hexdigest()


def install(store, source, expected=None):
    source = Path(source).resolve(strict=True)
    manifest = checked_manifest(source)
    digest = package_digest(source, manifest)
    if expected is not None and digest != expected: fail('Repair source no longer matches the pinned runtime identity')
    store = Path(store)
    store.mkdir(parents=True, exist_ok=True, mode=0o700)
    destination = store / digest
    if destination.exists():
        try:
            verify(destination)
            return {'digest': digest, 'manifest': manifest}
        except (ValueError, OSError): pass
    temporary = Path(tempfile.mkdtemp(prefix='.install-', dir=store))
    try:
        names = {item['source'] for item in manifest['files']} | set(manifest.get('registry', []))
        for name in sorted(names):
            output = temporary / relative(name)
            output.parent.mkdir(parents=True, exist_ok=True)
            # A real independent copy protects the global store from later source edits.
            with open(regular(source, name), 'rb') as src, open(output, 'xb') as dst:
                shutil.copyfileobj(src, dst); dst.flush(); os.fsync(dst.fileno())
            output.chmod(0o444)
        (temporary / 'manifest.json').write_bytes(canonical_json(manifest))
        (temporary / 'manifest.json').chmod(0o444)
        # Verify what was copied, not only the potentially mutable input directory.
        copied = checked_manifest(temporary)
        if package_digest(temporary, copied) != digest:
            fail('Runtime source changed while it was imported')
        for root, dirs, files in os.walk(temporary, topdown=False):
            for name in files:
                with open(Path(root) / name, 'rb') as stream: os.fsync(stream.fileno())
            os.chmod(root, 0o555)
        if destination.exists():
            # Replace a damaged generation, never mutate files used by running read-only mounts.
            quarantine = store / ('.quarantine-' + digest + '-' + str(time.time_ns()))
            os.rename(destination, quarantine)
        os.rename(temporary, destination)
        directory_fd = os.open(store, os.O_RDONLY | os.O_DIRECTORY)
        try: os.fsync(directory_fd)
        finally: os.close(directory_fd)
    except BaseException:
        if temporary.exists():
            for root, dirs, _ in os.walk(temporary): os.chmod(root, 0o700)
            shutil.rmtree(temporary)
        raise
    return {'digest': digest, 'manifest': manifest}


def verify(directory):
    manifest = checked_manifest(directory)
    digest = package_digest(directory, manifest)
    if directory.name != digest:
        fail('Runtime package identity does not match its content')
    return {'digest': digest, 'manifest': manifest}


def catalog(store, sources=None):
    root = Path(store)
    result = []
    for item in sorted(root.iterdir()) if root.exists() else []:
        if not re.fullmatch('[a-f0-9]{64}', item.name): continue
        try: result.append(verify(item))
        except (ValueError,OSError) as exc:
            try:
                if not sources or item.name not in sources: raise exc
                result.append(install(store, sources[item.name], expected=item.name))
            except (ValueError,OSError) as repair_error:
                result.append({'digest':item.name,'manifest':None,'error':str(repair_error)})
    return result


def inspect_binary(path):
    result = {'architecture':'unknown','imports':[],'frameworks':[],'managed':False}
    path = Path(path)
    try:
        with open(path, 'rb') as stream:
            if os.fstat(stream.fileno()).st_size < 64: return result
            with mmap.mmap(stream.fileno(), 0, access=mmap.ACCESS_READ) as data:
                def number(fmt, offset): return struct.unpack_from(fmt, data, offset)[0]
                if data[:2] != b'MZ': return result
                pe = number('<I', 60)
                if data[pe:pe+4] != b'PE\0\0': return result
                result['architecture'] = {0x14c:'x86',0x8664:'x64',0xaa64:'arm64'}.get(number('<H',pe+4),'unknown')
                count, optional = number('<H',pe+6), pe+24
                if count > 96: return result
                section = optional + number('<H',pe+20)
                magic = number('<H',optional)
                directory = optional + (96 if magic == 0x10b else 112 if magic == 0x20b else len(data))
                sections = [struct.unpack_from('<IIII',data,section+i*40+8) for i in range(count)]
                def rva(value):
                    for virtual_size, address, raw_size, raw in sections:
                        if address <= value < address + min(virtual_size or raw_size,raw_size): return raw+value-address
                    raise ValueError('PE RVA is outside file-backed sections')
                result['managed'] = bool(number('<I', directory+14*8))
                imported = number('<I',directory+8)
                if imported:
                    base = rva(imported)
                    for index in range(4096):
                        entry = struct.unpack_from('<IIIII',data,base+index*20)
                        if not any(entry): break
                        start = rva(entry[3]); end = data.find(b'\0',start,start+256)
                        if end < 0: break
                        result['imports'].append(data[start:end].decode('ascii').casefold())
    except (OSError,ValueError,struct.error,UnicodeError): pass
    sidecar = path.with_suffix('.runtimeconfig.json')
    if sidecar.is_file() and sidecar.stat().st_size <= MAX_REQUEST:
        try:
            options = json.loads(sidecar.read_bytes()).get('runtimeOptions',{})
            frameworks = options.get('frameworks',[]) + ([options['framework']] if 'framework' in options else [])
            result['frameworks'] = [f'{item["name"]}/{item["version"].split(".")[0]}' for item in frameworks if isinstance(item.get('name'),str) and isinstance(item.get('version'),str)]
        except (ValueError,TypeError,AttributeError,OSError): pass
    return result


def plan(store, profile, binaries, sources=None):
    for digest in profile.get('packages',[]):
        if not (Path(store)/digest).exists() and sources and digest in sources:
            install(store,sources[digest],expected=digest)
    packages = catalog(store, sources)
    pins = list(profile.get('packages',[]))
    for package in packages:
        if package['digest'] in pins and package['manifest'] is None:
            fail('Pinned runtime is damaged: '+package.get('error','unavailable'))
    for binary in binaries:
        inspected = inspect_binary(binary)
        matches = {}
        for package in packages:
            manifest = package['manifest']
            if manifest is None: continue
            if manifest['architecture'] not in ('any', inspected['architecture']): continue
            detect = manifest.get('detect',{})
            signals = ['dll:'+name.casefold() for name in detect.get('imports',[]) if name.casefold() in inspected['imports']]
            signals += ['framework:'+name for name in detect.get('frameworks',[]) if name in inspected['frameworks']]
            if detect.get('managed') and inspected['managed']: signals.append('managed')
            for signal_name in signals: matches.setdefault(signal_name,[]).append(package)
        for signal_name, providers in matches.items():
            chosen = [p for p in providers if p['digest'] in pins]
            chosen = chosen or [p for p in providers if p['manifest'].get('default') is True] or providers
            if len(chosen) != 1: fail(f'Automatic runtime detection for {signal_name} has ambiguous providers; a compatibility profile must pin one')
            if chosen[0]['digest'] not in pins: pins.append(chosen[0]['digest'])
    return resolve(store, profile.get('requirements',[]), pins, 'mixed')


def resolve(store, requirements, pins, architecture):
    packages = catalog(store)
    by_digest = {package['digest']: package for package in packages}
    selected, visiting = {}, set()

    def add(package):
        digest, manifest = package['digest'], package['manifest']
        if manifest is None: fail(f'Pinned runtime is damaged and cannot be repaired: {digest}: {package.get("error", "unavailable")}')
        if manifest['architecture'] not in (architecture, 'any') and architecture != 'mixed':
            fail(f'Runtime architecture mismatch: {manifest["id"]}')
        if digest in visiting: fail('Runtime dependency cycle')
        if digest in selected: return
        visiting.add(digest)
        for requirement in manifest.get('requires', []): require(requirement)
        visiting.remove(digest)
        selected[digest] = package

    def require(requirement):
        if any(requirement in p['manifest'].get('provides', []) for p in selected.values()): return
        candidates = [p for p in packages if p['manifest'] is not None and requirement in p['manifest'].get('provides', []) and (architecture == 'mixed' or p['manifest']['architecture'] in (architecture, 'any'))]
        # Versions are explicitly pinned when multiple providers exist: do not silently upgrade.
        pinned = [p for p in candidates if p['digest'] in pins]
        candidates = pinned or candidates
        if len(candidates) != 1:
            fail(f'Runtime requirement {requirement!r} needs exactly one compatible provider or an explicit package pin')
        add(candidates[0])

    for digest in pins:
        if digest not in by_digest: fail(f'Pinned runtime is unavailable: {digest}')
        add(by_digest[digest])
    for requirement in requirements: require(requirement)
    targets, overrides = {}, {}
    for package in selected.values():
        for item in package['manifest']['files']:
            target = item['target'].casefold()
            if target in targets and targets[target] != (package['digest'], item['source']):
                fail(f'Runtime files conflict at {item["target"]}')
            targets[target] = (package['digest'], item['source'])
        for dll, order in package['manifest'].get('dll_overrides', {}).items():
            if dll.casefold() in overrides and overrides[dll.casefold()] != order:
                fail(f'Runtime DLL overrides conflict for {dll}')
            overrides[dll.casefold()] = order
    return {'packages': list(selected.values()), 'dll_overrides': overrides}


def compose(config):
    prefix = Path(config['prefix']).resolve(strict=True)
    mounts, registry = [], []
    for package in config['packages']:
        directory = Path(config['store']) / package['digest']
        verified = verify(directory)
        if verified != package: fail('Runtime changed between resolution and composition')
        guest = '/run/vessel/packages/' + package['digest']
        mounts.append([str(directory), guest])
        for item in package['manifest']['files']:
            target = target_path(prefix, item['target'], allow_leaf_link=True)
            target.parent.mkdir(parents=True, exist_ok=True)
            if target.is_symlink(): target.unlink()  # Unlink the prefix alias, never follow its host target.
            if target.exists() and not target.is_file(): fail('Runtime mount target is not a file')
            if not target.exists(): target.touch(exist_ok=False)
            mounts.append([str(directory / item['source']), str(target)])
        registry.extend('Z:' + (guest + '/' + name).replace('/', '\\') for name in package['manifest'].get('registry', []))
    return {'mounts': mounts, 'registry': registry}


def send_frame(connection, kind, data):
    connection.sendall(kind + struct.pack('!I', len(data)) + data)


def receive(connection, size):
    data = bytearray()
    while len(data) < size:
        part = connection.recv(size - len(data))
        if not part: raise EOFError('Room supervisor disconnected')
        data.extend(part)
    return bytes(data)


def client(path, request):
    if not stat.S_ISSOCK(Path(path).lstat().st_mode): fail('Room control endpoint is not a socket')
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(5)
        connection.connect(str(path))
        peer, _, _ = struct.unpack('3i', connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize('3i')))
        if peer <= 0 or os.readlink(f'/proc/{peer}/ns/mnt') == os.readlink('/proc/self/ns/mnt'):
            fail('Room control peer is outside its sandbox')
        payload = canonical_json(request)
        if len(payload) > MAX_REQUEST: fail('Room command is too large')
        connection.sendall(struct.pack('!I', len(payload)) + payload)
        if not request.get('status'): connection.settimeout(None)
        while True:
            kind, size = receive(connection, 1), struct.unpack('!I', receive(connection, 4))[0]
            if size > MAX_REQUEST: fail('Invalid supervisor response')
            data = receive(connection, size)
            if kind == b'O': sys.stdout.buffer.write(data); sys.stdout.buffer.flush()
            elif kind == b'X': return int(data)
            else: fail('Invalid supervisor response')


def execute(connection, request):
    try:
        argv = request['argv']
        if not isinstance(argv, list) or not argv or not all(isinstance(arg, str) and '\0' not in arg for arg in argv): fail('Invalid room command')
        environment = dict(os.environ)
        environment.update(request.get('env', {}))
        child = subprocess.Popen(argv, cwd=request['cwd'], env=environment, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, close_fds=True)
        disconnected = False
        for block in iter(lambda: child.stdout.read1(65536), b''):
            if not disconnected:
                try: send_frame(connection, b'O', block)
                except OSError: disconnected = True
        code = child.wait()
        if not disconnected: send_frame(connection, b'X', str(code if code >= 0 else 128 - code).encode())
    except Exception as exc:
        try:
            send_frame(connection, b'O', f'Room launch failed: {exc}\n'.encode())
            send_frame(connection, b'X', b'125')
        except OSError: pass
    finally: connection.close()


def apps_running(children, supervisor):
    # Everything except the room's own plumbing (init, supervisor, display, bus) is an app.
    excluded = {1, supervisor} | {child.pid for child in children}
    for path in Path('/proc').iterdir():
        if not path.name.isdecimal() or int(path.name) in excluded: continue
        try:
            if (path / 'stat').read_text().rsplit(')', 1)[1].split()[0] != 'Z': return True
        except FileNotFoundError: pass
    return False


def processes_named(name):
    for path in Path('/proc').iterdir():
        try:
            if path.name.isdecimal() and (path / 'comm').read_text().strip() == name: return True
        except (FileNotFoundError, PermissionError): pass
    return False


def close_when_idle(children, supervisor, grace=5):
    # A room lives as long as its apps. Once one has run and none is left, close the room and its
    # display rather than leave an empty fullscreen window over the desktop.
    started = False; idle_since = None
    while True:
        time.sleep(1)
        if apps_running(children, supervisor):
            started = True; idle_since = None
        elif started:
            idle_since = idle_since or time.monotonic()
            if time.monotonic() - idle_since >= grace: os._exit(0)


def window_names():
    if not os.environ.get('DISPLAY'): return ''
    library = ctypes.util.find_library('X11')
    if not library: return ''
    x = ctypes.CDLL(library)
    x.XOpenDisplay.argtypes=[ctypes.c_char_p]; x.XOpenDisplay.restype=ctypes.c_void_p
    x.XDefaultRootWindow.argtypes=[ctypes.c_void_p]; x.XDefaultRootWindow.restype=ctypes.c_ulong
    x.XQueryTree.argtypes=[ctypes.c_void_p,ctypes.c_ulong,ctypes.POINTER(ctypes.c_ulong),ctypes.POINTER(ctypes.c_ulong),ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)),ctypes.POINTER(ctypes.c_uint)]
    x.XFetchName.argtypes=[ctypes.c_void_p,ctypes.c_ulong,ctypes.POINTER(ctypes.c_char_p)]
    x.XFree.argtypes=[ctypes.c_void_p]; x.XCloseDisplay.argtypes=[ctypes.c_void_p]
    display=x.XOpenDisplay(None)
    if not display: return ''
    names=[]; queue=[x.XDefaultRootWindow(display)]; visited=0
    try:
        while queue and visited<2048:
            window=queue.pop(); visited+=1; name=ctypes.c_char_p()
            if x.XFetchName(display,window,ctypes.byref(name)) and name.value:
                names.append(name.value.decode('utf-8','replace')); x.XFree(ctypes.cast(name,ctypes.c_void_p))
            root,parent=ctypes.c_ulong(),ctypes.c_ulong(); children=ctypes.POINTER(ctypes.c_ulong)(); count=ctypes.c_uint()
            if x.XQueryTree(display,window,ctypes.byref(root),ctypes.byref(parent),ctypes.byref(children),ctypes.byref(count)):
                queue.extend(children[index] for index in range(min(count.value,2048)))
                if children: x.XFree(children)
        return '\n'.join(names)
    finally: x.XCloseDisplay(display)


def supervise(config):
    # This branch runs inside bubblewrap. Never add host-side operations or a host-spawn RPC here.
    children = []
    if config.get('private_bus'):
        bus_config=Path('/run/vessel/ipc/dbus.conf')
        bus_config.write_text('<busconfig><type>session</type><listen>unix:path=/run/vessel/ipc/bus</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
        children.append(subprocess.Popen(['/usr/bin/dbus-daemon','--nofork','--nopidfile','--config-file='+str(bus_config)],stdin=subprocess.DEVNULL))
        deadline=time.monotonic()+10
        while not Path('/run/vessel/ipc/bus').exists():
            if children[-1].poll() is not None or time.monotonic()>deadline: fail('Private room D-Bus could not start')
            time.sleep(.03)
        os.environ['DBUS_SESSION_BUS_ADDRESS']='unix:path=/run/vessel/ipc/bus'
    if config['display'] == 'nested-x11':
        Path('/tmp/.X11-unix').mkdir(mode=0o700, exist_ok=True)
        # The X screen must be at least the game's resolution, or the area outside it is never redrawn.
        # Without -geometry, a fullscreen rootful Xwayland is 640x480 stretched over the monitor.
        screen = config.get('screen') or fail('Room display size is missing')
        size = ['-geometry', f"{int(screen['width'])}x{int(screen['height'])}", '-fullscreen' if screen['fullscreen'] else '-decorate']
        display = subprocess.Popen(['/usr/bin/Xwayland', ':0', '-nolisten', 'tcp', '-ac', *size], stdin=subprocess.DEVNULL)
        children.append(display)
        deadline = time.monotonic() + 15
        while not Path('/tmp/.X11-unix/X0').exists():
            if children[-1].poll() is not None or time.monotonic() > deadline: fail('Private Xwayland display could not start')
            time.sleep(0.05)
        os.environ['DISPLAY'] = ':0'
        # Closing the display window ends the room; a room without its display cannot run apps,
        # and the launcher opens a fresh room when this one has exited.
        threading.Thread(target=lambda: (display.wait(), os._exit(0)), daemon=True).start()
    if config.get('bootstrap'):
        result = subprocess.run(config['bootstrap'], stdin=subprocess.DEVNULL)
        if result.returncode: return result.returncode
        if config.get('wait_command'):
            result = subprocess.run(config['wait_command'], env=dict(os.environ, WINEPREFIX=config['wine_prefix']), stdin=subprocess.DEVNULL)
            if result.returncode: return result.returncode
    for command in config.get('registry_commands', []):
        result = subprocess.run(command, stdin=subprocess.DEVNULL)
        if result.returncode: fail('Windows runtime registry integration failed')
    if config.get('once'): return 0
    threading.Thread(target=close_when_idle, args=(children, os.getpid()), daemon=True).start()
    path = Path('/run/vessel/ipc/control.sock')
    path.unlink(missing_ok=True)
    with socket.socket(socket.AF_UNIX) as server:
        server.bind(str(path)); path.chmod(0o600); server.listen(32)
        while True:
            connection, _ = server.accept()
            try:
                size = struct.unpack('!I', receive(connection, 4))[0]
                if size > MAX_REQUEST: fail('Room request exceeds the size limit')
                request = json.loads(receive(connection, size))
                if request.get('diagnostics'):
                    processes = []
                    for process in Path('/proc').iterdir():
                        if not process.name.isdecimal(): continue
                        try: processes.append({'pid':int(process.name),'name':(process/'comm').read_text().strip()})
                        except (FileNotFoundError,PermissionError): pass
                    windows = window_names()
                    details={'processes':processes,'windows':windows,'mount_namespace':os.readlink('/proc/self/ns/mnt'),'network_namespace':os.readlink('/proc/self/ns/net')}
                    if request.get('screenshot') and os.environ.get('DISPLAY') and shutil.which('ffmpeg'):
                        captured=subprocess.run(['/usr/bin/ffmpeg','-loglevel','error','-f','x11grab','-draw_mouse','0','-i',os.environ['DISPLAY'],'-frames:v','1','-threads','1','-f','image2pipe','-vcodec','png','-'],capture_output=True,timeout=15)
                        if captured.returncode==0 and len(captured.stdout)<=8*MAX_REQUEST: details['image_base64']=base64.b64encode(captured.stdout).decode()
                        else: details['capture_error']=captured.stderr.decode(errors='replace')[-1000:]
                    payload=canonical_json(details)
                    for offset in range(0,len(payload),65536): send_frame(connection,b'O',payload[offset:offset+65536])
                    send_frame(connection,b'X',b'0'); connection.close(); continue
                if request.get('status'):
                    send_frame(connection, b'X', b'1' if apps_running(children, os.getpid()) else b'0')
                    connection.close(); continue
                if request.get('ready'):
                    # The Windows session is up once its wineserver runs; companions can join it.
                    send_frame(connection, b'X', b'0' if processes_named('wineserver') else b'1')
                    connection.close(); continue
                threading.Thread(target=execute, args=(connection, request), daemon=True).start()
            except Exception:
                connection.close()


def seccomp_fd():
    library = ctypes.util.find_library('seccomp')
    if not library: fail('libseccomp is required for secure rooms')
    seccomp = ctypes.CDLL(library)
    seccomp.seccomp_init.argtypes = [ctypes.c_uint32]; seccomp.seccomp_init.restype = ctypes.c_void_p
    seccomp.seccomp_syscall_resolve_name.argtypes = [ctypes.c_char_p]; seccomp.seccomp_syscall_resolve_name.restype = ctypes.c_int
    seccomp.seccomp_rule_add.argtypes = [ctypes.c_void_p, ctypes.c_uint32, ctypes.c_int, ctypes.c_uint]
    seccomp.seccomp_arch_add.argtypes = [ctypes.c_void_p, ctypes.c_uint32]
    seccomp.seccomp_export_bpf.argtypes = [ctypes.c_void_p, ctypes.c_int]
    seccomp.seccomp_release.argtypes = [ctypes.c_void_p]
    context = seccomp.seccomp_init(0x7fff0000)  # SCMP_ACT_ALLOW
    if not context: fail('Cannot create room seccomp policy')
    fd = os.memfd_create('vessel-seccomp', os.MFD_CLOEXEC)
    try:
        if os.uname().machine == 'x86_64':
            for architecture in (0x40000003, 0x4000003e):
                result = seccomp.seccomp_arch_add(context, architecture)
                if result not in (0, -errno.EEXIST): fail('Cannot protect Wine compatibility architectures')
        # Preserve ptrace/process_vm_* within the room for trainers/debuggers. PID namespaces
        # keep host targets out of reach. Nested mounts/user namespaces remain available to UMU.
        for name in ('add_key', 'request_key', 'keyctl', 'bpf', 'perf_event_open', 'kexec_load', 'kexec_file_load', 'reboot', 'swapon', 'swapoff', 'open_by_handle_at', 'init_module', 'finit_module', 'delete_module', 'userfaultfd'):
            number = seccomp.seccomp_syscall_resolve_name(name.encode())
            if number >= 0 and seccomp.seccomp_rule_add(context, 0x00050000 | errno.EPERM, number, 0): fail(f'Cannot filter {name}')
        if seccomp.seccomp_export_bpf(context, fd): fail('Cannot export room seccomp policy')
        os.lseek(fd, 0, os.SEEK_SET)
        return fd
    except BaseException:
        os.close(fd); raise
    finally: seccomp.seccomp_release(context)


def mount(args, source, target=None, writable=False, device=False):
    source = Path(source)
    if not source.exists(): fail(f'Required room resource is unavailable: {source}')
    args.extend(['--dev-bind' if device else '--bind' if writable else '--ro-bind', str(source), str(target or source)])


def target_path(prefix, name, allow_leaf_link=False):
    target = prefix
    parts=relative(name).parts
    for index, part in enumerate(parts):
        # Wine lookup is case insensitive. Resolve existing case to avoid shadow aliases.
        matches = [child for child in target.iterdir() if child.name.casefold() == part.casefold()] if target.is_dir() else []
        if len(matches) > 1: fail('Ambiguous case in Windows runtime destination')
        target = matches[0] if matches else target / part
        if target.is_symlink() and not (allow_leaf_link and index==len(parts)-1): fail('Windows runtime destination traverses a symlink')
    return target


def sandbox_args(config, descriptor, ids):
    # pasta runs its child as root in a new user namespace; map the room back to the host
    # user so UMU/Proton (which refuse root) and /run/user paths see the real identity.
    uid, gid = ids
    args = [shutil.which('bwrap') or fail('bubblewrap is required'), '--unshare-all', '--share-net', '--uid', str(uid), '--gid', str(gid), '--new-session', '--die-with-parent', '--cap-drop', 'ALL', '--clearenv', '--seccomp', str(descriptor)]
    mount(args, '/usr')
    for name in ('bin', 'sbin', 'lib', 'lib64'):
        path = Path('/') / name
        if path.is_symlink(): args.extend(['--symlink', os.readlink(path), str(path)])
        elif path.exists(): mount(args, path)
    args.extend(['--proc', '/proc', '--dev', '/dev', '--tmpfs', '/tmp', '--tmpfs', '/run', '--dir', '/etc', '--dir', '/run/vessel'])
    for name in ('ld.so.cache', 'hosts', 'nsswitch.conf', 'localtime', 'fonts', 'ssl/certs', 'vulkan', 'glvnd', 'alternatives'):
        path = Path('/etc') / name
        if path.exists(): mount(args, path)
    resolver = Path(config['guest_config']).parent / 'room-resolv.conf'
    resolver.write_text('nameserver 169.254.1.1\n' if config['network'] == 'online' else '')
    mount(args, resolver, '/etc/resolv.conf')
    # Minimal account metadata is generated by the trusted host, not inherited home data.
    mount(args, config['accounts'], '/etc/passwd')
    mount(args, config['engine'], '/run/vessel/engine.py')
    mount(args, config['guest_config'], '/run/vessel/config.json')
    mount(args, config['home'], config['guest_home'], writable=True)
    mount(args, config['ipc'], '/run/vessel/ipc', writable=True)
    for path in config['writable_roots']: mount(args, path, writable=True)
    for path in config['read_only_roots']: mount(args, path)
    for source, destination in config.get('runtime_mounts', []): mount(args, source, destination)
    for source, destination in config.get('private_mounts', []): mount(args, source, destination, writable=True)
    runtime = f'/run/user/{uid}'
    args.extend(['--dir', runtime])
    if config['display'] != 'headless':
        mount(args, config['wayland'], runtime + '/wayland-0')
    if config.get('audio'):
        pulse = Path(os.environ.get('XDG_RUNTIME_DIR', f'/run/user/{uid}')) / 'pulse/native'
        if pulse.exists(): mount(args, pulse, runtime + '/pulse/native')
    if config.get('graphics'):
        for path in sorted(Path('/dev/dri').glob('renderD*')) if Path('/dev/dri').exists() else []:
            mount(args, path, device=True)
        for name in ('nvidiactl', 'nvidia-uvm', 'nvidia0', 'nvidia1', 'nvidia-modeset'):
            path = Path('/dev') / name
            if path.exists(): mount(args, path, device=True)
        for name in ('devices', 'class/drm', 'bus/pci', 'dev/char'):
            path = Path('/sys') / name
            if path.exists(): mount(args, path)
    if config.get('controllers'):
        for link in Path('/dev/input/by-id').glob('*-joystick'):
            device = link.resolve()
            if device.parent == Path('/dev/input') and stat.S_ISCHR(device.stat().st_mode): mount(args, device, device=True)
    environment = {'HOME': config['guest_home'], 'USER': config['user'], 'LOGNAME': config['user'], 'PATH': '/usr/local/bin:/usr/bin:/bin', 'LANG': os.environ.get('LANG', 'C.UTF-8'), 'XDG_RUNTIME_DIR': runtime, 'XDG_CACHE_HOME': config['guest_home'] + '/.cache', 'XDG_CONFIG_HOME': config['guest_home'] + '/.config', 'XDG_DATA_HOME': config['guest_home'] + '/.local/share', 'WINEPREFIX': config['prefix'], 'UMU_RUNTIME_UPDATE': '0', 'PYTHONNOUSERSITE': '1'}
    if config['display'] != 'headless': environment['WAYLAND_DISPLAY'] = 'wayland-0'
    if config['display'] == 'wayland': environment['PROTON_ENABLE_WAYLAND'] = '1'
    if config.get('audio'): environment['PULSE_SERVER'] = 'unix:' + runtime + '/pulse/native'
    environment.update(config.get('environment', {}))
    for name, value in environment.items(): args.extend(['--setenv', name, str(value)])
    args.extend(['--chdir', config['guest_home'], '--', '/usr/bin/python3', '-I', '/run/vessel/engine.py', 'supervise', '/run/vessel/config.json'])
    if config['network'] != 'online':
        # Without pasta there is no parent private network namespace to retain.
        args.remove('--share-net')
    return args


def session(config, config_path, network_ready=False, ids=None):
    ids = ids or (os.getuid(), os.getgid())
    descriptor = None
    child = None
    def stop(signum, frame):
        if child is not None and child.poll() is None:
            try: os.killpg(child.pid, signal.SIGTERM)
            except ProcessLookupError: pass
        raise SystemExit(128 + signum)
    signal.signal(signal.SIGTERM, stop); signal.signal(signal.SIGINT, stop)
    try:
        if config['network'] == 'online' and not network_ready:
            pasta = shutil.which('pasta') or fail('Online rooms require pasta (the passt package)')
            nameservers = [line.split()[1] for line in Path('/etc/resolv.conf').read_text().splitlines() if line.startswith('nameserver ') and len(line.split()) >= 2]
            dns = ['--dns-host', nameservers[0]] if nameservers else []
            args = [pasta, '--foreground', '--config-net', '--no-map-gw', '--dns-forward', '169.254.1.1', *dns, '-t', 'none', '-u', 'none', '-T', 'none', '-U', 'none', '--', sys.executable, '-I', str(Path(__file__).resolve()), 'network-child', str(config_path), str(ids[0]), str(ids[1])]
            # pasta manages descriptors for its own tap sockets. Generate the seccomp FD
            # after its exec, instead of allowing an inherited FD number to be repurposed.
            child = subprocess.Popen(args, start_new_session=True, close_fds=True)
        else:
            descriptor = seccomp_fd()
            args = sandbox_args(config, descriptor, ids)
            child = subprocess.Popen(args, pass_fds=(descriptor,), start_new_session=True, close_fds=True)
        return child.wait()
    finally:
        if descriptor is not None: os.close(descriptor)
        if child is not None and child.poll() is None:
            try: os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError: pass
            child.wait()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=['install', 'catalog', 'resolve', 'plan', 'compose', 'session', 'network-child', 'supervise', 'exec'])
    parser.add_argument('arguments', nargs='*')
    options = parser.parse_args()
    a = options.arguments
    if options.action == 'install': print(json.dumps(install(a[0], a[1], a[2] if len(a)>2 else None)))
    elif options.action == 'catalog': print(json.dumps(catalog(a[0])))
    elif options.action == 'resolve': print(json.dumps(resolve(a[0], json.loads(a[1]), json.loads(a[2]), a[3])))
    elif options.action == 'compose': print(json.dumps(compose(json.loads(a[0]))))
    elif options.action == 'plan': print(json.dumps(plan(a[0],json.loads(a[1]),json.loads(a[2]),json.loads(a[3]) if len(a)>3 else None)))
    elif options.action == 'session': return session(json.loads(Path(a[0]).read_bytes()), a[0])
    elif options.action == 'network-child': return session(json.loads(Path(a[0]).read_bytes()), a[0], network_ready=True, ids=(int(a[1]), int(a[2])))
    elif options.action == 'supervise': return supervise(json.loads(Path(a[0]).read_bytes()))
    elif options.action == 'exec': return client(a[0], json.loads(a[1]))
    return 0


if __name__ == '__main__':
    try: sys.exit(main())
    except Exception as exc:
        print(f'Vessel room: {exc}', file=sys.stderr)
        sys.exit(125)
