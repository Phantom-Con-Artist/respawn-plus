# SPDX-License-Identifier: MIT
"""Run with python3 -m unittest discover -s tests -p 'test_room_engine.py'.

These exercise real mount/PID/network namespaces, not just generated command strings.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import socket
import struct
import sys
import tempfile
import time
import unittest

ENGINE = Path(__file__).resolve().parents[1] / 'src-tauri/room_engine/engine.py'
spec = importlib.util.spec_from_file_location('room_engine', ENGINE)
engine = importlib.util.module_from_spec(spec)
spec.loader.exec_module(engine)


def make_package(root, name='example-runtime', version='1.0', provides=None, target='drive_c/runtime/example.bin'):
    root.mkdir(parents=True)
    (root / 'runtime.bin').write_bytes(b'shared runtime bytes')
    (root / 'runtime.reg').write_text('Windows Registry Editor Version 5.00\n[HKEY_CURRENT_USER\\Software\\VesselExample]\n"Ready"="yes"\n')
    manifest = {'schema':1, 'id':name, 'version':version, 'architecture':'any', 'provides':provides or ['example-runtime'], 'files':[{'source':'runtime.bin','target':target,'sha256':hashlib.sha256((root / 'runtime.bin').read_bytes()).hexdigest()}], 'registry':['runtime.reg']}
    (root / 'manifest.json').write_text(json.dumps(manifest))
    return manifest


class Packages(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='vessel-packages-')
        self.root = Path(self.temp.name)
        self.store = self.root / 'store'
        self.source = self.root / 'source'
        make_package(self.source)

    def tearDown(self):
        if self.store.exists():
            for root, dirs, _ in os.walk(self.store): os.chmod(root, 0o700)
        self.temp.cleanup()

    def test_content_identity_deduplication_source_independence_and_registry_identity(self):
        first = engine.install(self.store, self.source)
        self.assertEqual(first, engine.install(self.store, self.source))
        self.assertEqual(len(list(self.store.iterdir())), 1)
        (self.source / 'runtime.bin').write_bytes(b'changed installer files')
        self.assertEqual((self.store / first['digest'] / 'runtime.bin').read_bytes(), b'shared runtime bytes')
        (self.source / 'runtime.bin').write_bytes(b'shared runtime bytes')
        (self.source / 'runtime.reg').write_text('different registry recipe')
        second = engine.install(self.store, self.source)
        self.assertNotEqual(first['digest'], second['digest'])

    def test_rejects_traversal_symlinks_and_bad_checksums(self):
        manifest = json.loads((self.source / 'manifest.json').read_text())
        for path in ('../host', '/etc/passwd', 'drive_c/../../etc/passwd', 'drive_c/C:/escape'):
            manifest['files'][0]['target'] = path
            (self.source / 'manifest.json').write_text(json.dumps(manifest))
            with self.assertRaises(ValueError): engine.install(self.store, self.source)
        manifest['files'][0]['target'] = 'drive_c/runtime/example.bin'
        (self.source / 'manifest.json').write_text(json.dumps(manifest))
        (self.source / 'runtime.bin').unlink()
        (self.source / 'runtime.bin').symlink_to('/etc/passwd')
        with self.assertRaises(ValueError): engine.install(self.store, self.source)
        (self.source / 'runtime.bin').unlink(); (self.source / 'runtime.bin').write_bytes(b'corrupt')
        with self.assertRaises(ValueError): engine.install(self.store, self.source)

    def test_provider_resolution_pins_dependencies_and_conflicts(self):
        first = engine.install(self.store, self.source)
        second_source = self.root / 'second'
        make_package(second_source, version='2.0')
        second = engine.install(self.store, second_source)
        with self.assertRaisesRegex(ValueError, 'exactly one'): engine.resolve(self.store, ['example-runtime'], [], 'mixed')
        result = engine.resolve(self.store, ['example-runtime'], [first['digest']], 'mixed')
        self.assertEqual([p['digest'] for p in result['packages']], [first['digest']])
        with self.assertRaisesRegex(ValueError, 'conflict'): engine.resolve(self.store, [], [first['digest'], second['digest']], 'mixed')
        dependency = self.root / 'dependency'
        manifest = make_package(dependency, name='dependent', provides=['dependent'], target='drive_c/runtime/dependent.bin')
        manifest['requires'] = ['example-runtime']
        (dependency / 'manifest.json').write_text(json.dumps(manifest))
        dep = engine.install(self.store, dependency)
        result = engine.resolve(self.store, ['dependent'], [first['digest'], dep['digest']], 'mixed')
        self.assertEqual(len(result['packages']), 2)

    def test_composition_does_not_copy_payload_and_rejects_prefix_links(self):
        package = engine.install(self.store, self.source)
        prefix = self.root / 'prefix'; prefix.mkdir()
        composed = engine.compose({'prefix':str(prefix),'packages':[package],'store':str(self.store)})
        self.assertEqual((prefix / 'drive_c/runtime/example.bin').read_bytes(), b'')
        self.assertEqual(len(composed['mounts']), 2)
        self.assertTrue(composed['registry'][0].startswith('Z:\\run\\vessel\\packages'))
        (prefix / 'drive_c/runtime/example.bin').unlink()
        (prefix / 'drive_c/runtime/example.bin').symlink_to('/etc/passwd')
        original=Path('/etc/passwd').read_bytes()
        engine.compose({'prefix':str(prefix),'packages':[package],'store':str(self.store)})
        self.assertEqual(Path('/etc/passwd').read_bytes(),original)
        self.assertFalse((prefix/'drive_c/runtime/example.bin').is_symlink())
        shutil.rmtree(prefix/'drive_c/runtime')
        (prefix/'drive_c/runtime').symlink_to(self.root)
        with self.assertRaisesRegex(ValueError, 'symlink'): engine.compose({'prefix':str(prefix),'packages':[package],'store':str(self.store)})

    def test_poisoned_runtime_is_repaired_only_from_the_original_pinned_content(self):
        package=engine.install(self.store,self.source)
        payload=self.store/package['digest']/'runtime.bin'
        payload.chmod(0o644); payload.write_bytes(b'poisoned')
        profile={'packages':[package['digest']]}
        with self.assertRaisesRegex(ValueError,'damaged'): engine.plan(self.store,profile,[])
        result=engine.plan(self.store,profile,[],{package['digest']:str(self.source)})
        self.assertEqual(result['packages'][0]['digest'],package['digest'])
        self.assertEqual(payload.read_bytes(),b'shared runtime bytes')
        payload.chmod(0o644); payload.write_bytes(b'poisoned again')
        (self.source/'runtime.reg').write_text('untrusted changed recipe')
        with self.assertRaisesRegex(ValueError,'pinned runtime identity'): engine.plan(self.store,profile,[],{package['digest']:str(self.source)})

    def test_pe_import_detection_is_agnostic_and_pins_a_matching_provider(self):
        data=bytearray(2048); data[:2]=b'MZ'; struct.pack_into('<I',data,60,128)
        data[128:132]=b'PE\0\0'; struct.pack_into('<H',data,132,0x8664); struct.pack_into('<H',data,134,1)
        struct.pack_into('<H',data,148,240); struct.pack_into('<H',data,152,0x20b)
        struct.pack_into('<I',data,152+112+8,0x1000)
        struct.pack_into('<IIII',data,152+240+8,0x1000,0x1000,1024,512)
        struct.pack_into('<I',data,512+12,0x1040); data[576:594]=b'vcruntime140.dll\0\0\0'
        binary=self.root/'AnyGame.exe'; binary.write_bytes(data)
        manifest=json.loads((self.source/'manifest.json').read_text()); manifest['detect']={'imports':['vcruntime140.dll']}
        (self.source/'manifest.json').write_text(json.dumps(manifest)); installed=engine.install(self.store,self.source)
        inspected=engine.inspect_binary(binary)
        self.assertEqual(inspected['architecture'],'x64'); self.assertIn('vcruntime140.dll',inspected['imports'])
        self.assertEqual(engine.plan(self.store,{},[binary])['packages'][0]['digest'],installed['digest'])
        binary.write_bytes(b'MZ'+b'\xff'*128)
        self.assertEqual(engine.inspect_binary(binary)['imports'],[])

    def test_host_control_sockets_and_symlink_endpoints_are_rejected(self):
        path=self.root/'host.sock'
        with socket.socket(socket.AF_UNIX) as server:
            server.bind(str(path)); server.listen(1)
            with self.assertRaisesRegex(ValueError,'outside its sandbox'): engine.client(path,{'status':True})
            alias=self.root/'alias'; alias.symlink_to(path)
            with self.assertRaisesRegex(ValueError,'not a socket'): engine.client(alias,{'status':True})


class Sandbox(unittest.TestCase):
    def setUp(self):
        if not shutil.which('bwrap'): self.skipTest('bubblewrap not installed')
        self.temp = tempfile.TemporaryDirectory(prefix='vessel-sandbox-')
        self.root = Path(self.temp.name)
        self.processes = []

    def tearDown(self):
        for process in self.processes:
            process.terminate()
            try: process.wait(timeout=5)
            except subprocess.TimeoutExpired: process.kill(); process.wait()
            process.stderr.close()
        if hasattr(self, 'temp'): self.temp.cleanup()

    def start(self, name, mounts=None, network='offline', display='headless', screen=None):
        root = self.root / name; root.mkdir()
        for directory in ('home','ipc','prefix','game'): (root / directory).mkdir()
        accounts = root / 'passwd'; accounts.write_text(f'player:x:{os.getuid()}:{os.getgid()}:Room:/home/player:/bin/sh\n')
        guest = root / 'guest.json'; guest.write_text(json.dumps({'display':display,'screen':screen,'once':False}))
        config = {'engine':str(ENGINE),'accounts':str(accounts),'guest_config':str(guest),'home':str(root/'home'),'guest_home':'/home/player','ipc':str(root/'ipc'),'prefix':str(root/'prefix'),'user':'player','writable_roots':[str(root/'prefix'),str(root/'game')],'read_only_roots':[],'runtime_mounts':mounts or [],'display':display,'graphics':False,'audio':False,'controllers':False,'network':network}
        if display != 'headless': config['wayland'] = str(Path(os.environ['XDG_RUNTIME_DIR']) / os.environ['WAYLAND_DISPLAY'])
        config_path = root / 'config.json'; config_path.write_text(json.dumps(config))
        environment = dict(os.environ, VESSEL_HOST_SECRET='never-inherit-this')
        process = subprocess.Popen([sys.executable,'-I',str(ENGINE),'session',str(config_path)], env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        self.processes.append(process)
        deadline = time.monotonic() + 20
        while not (root/'ipc/control.sock').exists():
            if process.poll() is not None: self.fail(process.stderr.read().decode())
            if time.monotonic() > deadline: self.fail('Sandbox supervisor startup timed out')
            time.sleep(.03)
        return root

    def run_in(self, root, source):
        request = {'argv':['/usr/bin/python3','-I','-c',source], 'cwd':str(root/'game'), 'env':{}}
        result = subprocess.run([sys.executable,'-I',str(ENGINE),'exec',str(root/'ipc/control.sock'),json.dumps(request)], capture_output=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stdout.decode()+result.stderr.decode())
        return result.stdout.decode()

    def test_private_home_processes_network_environment_and_real_readonly_mounts(self):
        shared = self.root / 'runtime.bin'; shared.write_bytes(b'one global copy')
        secret = self.root / 'personal-secret'; secret.write_text('host secret')
        first = self.start('first', [[str(shared),'/opt/runtime.bin']])
        second = self.start('second', [[str(shared),'/opt/runtime.bin']])
        source = f'''import os,json,pathlib,socket,ctypes,errno
p=pathlib.Path('/opt/runtime.bin')
try: p.write_bytes(b'bad'); writable=True
except OSError: writable=False
libc=ctypes.CDLL(None,use_errno=True)
blocked=libc.syscall(250,0)==-1 and ctypes.get_errno()==errno.EPERM
print(json.dumps({{'bytes':p.read_text(),'inode':p.stat().st_ino,'writable':writable,'secret':pathlib.Path({str(secret)!r}).exists(),'other_room':pathlib.Path({str(second/'prefix')!r}).exists(),'env_secret':os.environ.get('VESSEL_HOST_SECRET'),'host_pid':pathlib.Path('/proc/{os.getpid()}').exists(),'network':os.readlink('/proc/self/ns/net'),'mount':os.readlink('/proc/self/ns/mnt'),'keyctl_blocked':blocked}}))'''
        a = json.loads(self.run_in(first, source)); b = json.loads(self.run_in(second, source))
        self.assertEqual(a['inode'], shared.stat().st_ino); self.assertEqual(a['inode'],b['inode'])
        self.assertFalse(a['writable']); self.assertFalse(a['secret']); self.assertFalse(a['other_room'])
        self.assertIsNone(a['env_secret']); self.assertFalse(a['host_pid'])
        if os.uname().machine == 'x86_64': self.assertTrue(a['keyctl_blocked'])
        self.assertNotEqual(a['network'], os.readlink('/proc/self/ns/net')); self.assertNotEqual(a['network'],b['network'])
        self.assertNotEqual(a['mount'],b['mount']); self.assertEqual(shared.read_bytes(),b'one global copy')
        self.assertEqual(self.run_in(first,"from pathlib import Path; Path('/home/player/setting').write_text('private')"), '')
        self.assertEqual((first/'home/setting').read_text(),'private')
        self.assertEqual(self.run_in(second,"from pathlib import Path; print(Path('/home/player/setting').exists())").strip(),'False')

    def test_companions_share_namespaces_and_status_detects_running_descendants(self):
        root = self.start('companions')
        first = self.run_in(root,"import os; print(os.readlink('/proc/self/ns/mnt'))")
        second = self.run_in(root,"import os; print(os.readlink('/proc/self/ns/mnt'))")
        self.assertEqual(first,second)
        self.run_in(root,"import subprocess; subprocess.Popen(['/usr/bin/sleep','1'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)")
        args = [sys.executable,'-I',str(ENGINE),'exec',str(root/'ipc/control.sock'),'{"status":true}']
        self.assertEqual(subprocess.run(args).returncode,1)
        time.sleep(1.1)
        self.assertEqual(subprocess.run(args).returncode,0)

    def test_room_closes_itself_once_its_last_app_has_ended(self):
        # An idle room must not keep its fullscreen display over the desktop.
        root = self.start('idle')
        time.sleep(6)
        self.assertIsNone(self.processes[-1].poll(), 'a room with no app yet stays open for its first launch')
        self.run_in(root, "import subprocess; subprocess.Popen(['/usr/bin/sleep','2'])")
        time.sleep(4)
        self.assertIsNone(self.processes[-1].poll(), 'a detached app keeps the room open after its launcher exits')
        self.assertIsNotNone(self.processes[-1].wait(timeout=15))

    def test_online_network_uses_a_private_namespace(self):
        if not shutil.which('pasta'): self.skipTest('pasta not installed')
        root = self.start('online', network='online')
        net = self.run_in(root,"import os; print(os.readlink('/proc/self/ns/net'))").strip()
        self.assertNotEqual(net, os.readlink('/proc/self/ns/net'))

    def test_online_and_offline_rooms_run_as_the_host_user_not_root(self):
        # UMU refuses to start as root; pasta's user namespace would otherwise map to uid 0.
        for network in ['offline']+(['online'] if shutil.which('pasta') else []):
            root = self.start('identity-'+network, network=network)
            ids = self.run_in(root,"import os; print(os.geteuid(), os.getegid(), os.environ['XDG_RUNTIME_DIR'])").split()
            self.assertEqual(ids, [str(os.getuid()), str(os.getgid()), f'/run/user/{os.getuid()}'])

    def test_online_and_offline_rooms_cannot_connect_to_host_abstract_sockets(self):
        address='\0vessel-host-only-'+str(os.getpid())
        with socket.socket(socket.AF_UNIX) as server:
            server.bind(address); server.listen(1)
            for network in ['offline']+(['online'] if shutil.which('pasta') else []):
                root=self.start('abstract-'+network,network=network)
                result=self.run_in(root,f"import socket; s=socket.socket(socket.AF_UNIX); s.settimeout(1)\ntry: s.connect({address!r}); print('HOST REACHED')\nexcept OSError: print('isolated')")
                self.assertEqual(result.strip(),'isolated')

    def test_private_xwayland_starts_without_host_x11_access(self):
        if not os.environ.get('WAYLAND_DISPLAY') or not shutil.which('Xwayland'): self.skipTest('Wayland/Xwayland unavailable')
        root = self.start('display', display='nested-x11', screen={'fullscreen':False,'width':800,'height':600})
        details = json.loads(self.run_in(root,"import os,json,pathlib; print(json.dumps({'display':os.environ.get('DISPLAY'),'xauth':os.environ.get('XAUTHORITY'),'socket':pathlib.Path('/tmp/.X11-unix/X0').exists()}))"))
        self.assertEqual(details,{'display':':0','xauth':None,'socket':True})

    def test_private_xwayland_screen_matches_the_game_resolution(self):
        # A game larger than the X screen leaves the rest of its window undrawn.
        if not os.environ.get('WAYLAND_DISPLAY') or not shutil.which('Xwayland'): self.skipTest('Wayland/Xwayland unavailable')
        root = self.start('screen', display='nested-x11', screen={'fullscreen':False,'width':1024,'height':640})
        size = self.run_in(root,"import ctypes; x=ctypes.CDLL('libX11.so.6'); x.XOpenDisplay.restype=ctypes.c_void_p; x.XDisplayWidth.argtypes=x.XDisplayHeight.argtypes=[ctypes.c_void_p,ctypes.c_int]; d=x.XOpenDisplay(None); print(x.XDisplayWidth(d,0), x.XDisplayHeight(d,0))")
        self.assertEqual(size.split(), ['1024','640'])

    def test_closing_the_display_ends_the_room(self):
        # A room without its display cannot run apps; it must exit so the launcher opens a new one.
        if not os.environ.get('WAYLAND_DISPLAY') or not shutil.which('Xwayland'): self.skipTest('Wayland/Xwayland unavailable')
        root = self.start('closed', display='nested-x11', screen={'fullscreen':False,'width':800,'height':600})
        self.run_in(root,"import os,pathlib,signal; [os.kill(int(p.name),signal.SIGTERM) for p in pathlib.Path('/proc').iterdir() if p.name.isdecimal() and (p/'comm').read_text().strip()=='Xwayland']")
        self.assertIsNotNone(self.processes[-1].wait(timeout=10))

    @unittest.skipUnless(os.environ.get('VESSEL_TEST_WINE') == '1', 'set VESSEL_TEST_WINE=1 for real Wine smoke tests')
    def test_real_wine_console_in_an_isolated_prefix(self):
        root = self.start('wine')
        wine = str(Path(shutil.which('wine')).resolve())
        request = {'argv':[wine,'cmd.exe','/c','echo VESSEL_WINE_OK'], 'cwd':str(root/'game'), 'env':{'WINEDEBUG':'-all'}}
        result = subprocess.run([sys.executable,'-I',str(ENGINE),'exec',str(root/'ipc/control.sock'),json.dumps(request)], capture_output=True, timeout=90)
        self.assertEqual(result.returncode,0,result.stdout.decode()+result.stderr.decode())
        self.assertIn(b'VESSEL_WINE_OK',result.stdout)


if __name__ == '__main__': unittest.main()
