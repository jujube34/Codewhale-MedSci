"""Stage the official, pinned Git for Windows portable distribution (including Bash)."""
import hashlib,json,os,pathlib,shutil,subprocess,urllib.request
root=pathlib.Path(__file__).resolve().parents[2]
version='2.55.0.windows.5'
name='PortableGit-2.55.0.5-64-bit.7z.exe'
url=f'https://github.com/git-for-windows/git/releases/download/v{version}/{name}'
sha='5aa8a20f6e9abb2c755f0e73c91c687701a46b309ad84a0ca6509380fa4ae290'
cache=root/'desktop/runtime/git-bash';cache.mkdir(parents=True,exist_ok=True)
archive=cache/name
if not archive.exists():urllib.request.urlretrieve(url,archive)
assert hashlib.sha256(archive.read_bytes()).hexdigest()==sha,'Git for Windows archive checksum mismatch'
stage=cache/'payload'
if stage.exists():shutil.rmtree(stage)
sevenzip=shutil.which('7z') or str(pathlib.Path(os.environ.get('ProgramFiles','C:/Program Files'))/'7-Zip/7z.exe')
subprocess.run([sevenzip,'x','-y',f'-o{stage}',str(archive)],check=True,stdout=subprocess.DEVNULL)
for binary in ['bin/bash.exe','usr/bin/bash.exe','cmd/git.exe','usr/bin/msys-2.0.dll']:
 assert (stage/binary).is_file(),binary
manifest={'version':version,'source':url,'archive_sha256':sha,'files':{str(p.relative_to(stage)).replace('\\','/'):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(stage.rglob('*')) if p.is_file()}}
(stage/'medsci-git-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
print(f'PASS: verified official PortableGit {version}; staged {len(manifest["files"])} files including Bash, Git and original licenses')
