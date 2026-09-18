"""Prepare a target-native, offline Python payload. Run on each build runner."""
import json,os,pathlib,platform,shutil,subprocess,sys
root=pathlib.Path(__file__).resolve().parents[2];runtime=root/'desktop/runtime';dest=root/'apps/desktop/src-tauri/resources/python'
def run(*args):subprocess.run([str(a) for a in args],check=True)
run('uv','python','install','3.12.13','--no-bin','--install-dir',runtime/'cpython')
platform_name='windows' if os.name=='nt' else 'macos'
arch='aarch64' if platform.machine().lower() in ('arm64','aarch64') else 'x86_64'
base=runtime/'cpython'/f'cpython-3.12.13-{platform_name}-{arch}-none'
python=base/('python.exe' if os.name=='nt' else 'bin/python3')
wheels=runtime/'wheelhouse';wheels.mkdir(exist_ok=True)
run(python,'-m','pip','wheel','antlr4-python3-runtime==4.9.3','--wheel-dir',wheels)
pinned=(runtime/'requirements.pinned').read_text()
if platform_name=='macos' and arch=='x86_64':pinned=pinned.replace('onnxruntime==1.30.0','onnxruntime==1.23.2').replace('opencv-python==5.0.0.93','opencv-python==4.12.0.88').replace('numpy==2.5.3','numpy==2.2.6')
(runtime/'target-requirements.txt').write_text(pinned)
run(python,'-m','pip','download','--only-binary=:all:','--find-links',wheels,'-r',runtime/'target-requirements.txt','-d',wheels)
run(sys.executable,root/'desktop/scripts/lock-wheels.py')
dest.mkdir(parents=True,exist_ok=True)
for name in ['manager.py','ocr.py','requirements.lock','wheelhouse-manifest.json']:shutil.copy2(runtime/name,dest/name)
for name,source in [('cpython',base),('wheelhouse',wheels)]:
 if (dest/name).exists():shutil.rmtree(dest/name)
 shutil.copytree(source,dest/name,symlinks=True)
# Use the installed MSVC redistributable CRT for native Windows builds.
if os.name=='nt':
 run(sys.executable,root/'desktop/scripts/prepare-git-bash.py')
 git_dest=dest.parent/'git-bash'
 if git_dest.exists():shutil.rmtree(git_dest)
 shutil.copytree(runtime/'git-bash/payload',git_dest)
 import hashlib
 vswhere=pathlib.Path(os.environ['ProgramFiles(x86)'])/'Microsoft Visual Studio/Installer/vswhere.exe'
 vs=pathlib.Path(subprocess.check_output([str(vswhere),'-latest','-products','*','-property','installationPath'],text=True).strip())
 candidates=list((vs/'VC/Redist/MSVC').glob('*/x64/Microsoft.VC*.CRT'))
 if not candidates:raise RuntimeError('MSVC x64 redistributable CRT not found')
 crt=max(candidates,key=lambda p:tuple(int(n) for n in p.parents[1].name.split('.')))
 target=dest/'vc-runtime';target.mkdir(exist_ok=True)
 hashes={}
 for dll in crt.glob('*.dll'):
  shutil.copy2(dll,target/dll.name);shutil.copy2(dll,dest/'cpython'/dll.name)
  hashes[dll.name]=hashlib.sha256(dll.read_bytes()).hexdigest()
 assert 'msvcp140.dll' in hashes
 (target/'manifest.json').write_text(json.dumps({'source':str(crt),'architecture':'x64','files':hashes},indent=2),encoding='utf-8')
# Validate the actual interpreter embedded in the staged payload.
embedded=dest/'cpython'/('python.exe' if os.name=='nt' else 'bin/python3')
run(embedded,'-I',dest/'manager.py','--home',root/'desktop/test-runtime','initialize')
shared=root/'desktop/test-runtime/current'/('Scripts/python.exe' if os.name=='nt' else 'bin/python3')
run(shared,'-I',root/'desktop/tests/document_smoke.py')
