"""Build an unsigned current-user NSIS installer from the cross-built payload."""
import argparse,hashlib,json,os,pathlib,shutil,subprocess
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--version',default='0.1.0-preview.7.5')
parser.add_argument('--desktop-exe',type=pathlib.Path)
parser.add_argument('--sidecar-exe',type=pathlib.Path)
parser.add_argument('--makensis',default='makensis')
args=parser.parse_args()
if not args.version or any(c not in 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.-' for c in args.version):parser.error('Invalid version')
root=pathlib.Path(__file__).resolve().parents[2];artifacts=root/'desktop/artifacts';artifacts.mkdir(exist_ok=True)
stage=artifacts/f'stage-windows-x86_64-{args.version}'
# Keep earlier packages intact; choose a new version for a new payload.
if stage.exists():parser.error(f'Staging directory already exists: {stage}')
stage.mkdir()
exe=args.desktop_exe or root/'apps/desktop/target/x86_64-pc-windows-msvc/release/medsci-desktop.exe'
agent=args.sidecar_exe or root/'target-windows/x86_64-pc-windows-msvc/release/codewhale.exe'
shutil.copy2(exe,stage/exe.name)
resources=stage/'resources';resources.mkdir();shutil.copy2(agent,resources/'codewhale.exe')
runtime=root/'desktop/runtime';target=runtime/'windows-x86_64';payload=resources/'python';payload.mkdir()
git_bash=runtime/'git-bash/payload'
assert (git_bash/'bin/bash.exe').is_file(), 'Run prepare-git-bash.py first'
shutil.copytree(git_bash,resources/'git-bash')
for name in ['manager.py','ocr.py','ocr-models-manifest.json']:shutil.copy2(runtime/name,payload/name)
for name in ['requirements.lock','wheelhouse-manifest.json']:shutil.copy2(target/name,payload/name)
shutil.copytree(target/'wheelhouse',payload/'wheelhouse')
shutil.copytree(runtime/'cpython/windows-payload/python',payload/'cpython',ignore=shutil.ignore_patterns('__pycache__','*.pyc','*.pyo'))
# Bundle the complete x64 C++ CRT alongside every native executable.
crt=target/'vc-runtime';shutil.copytree(crt,payload/'vc-runtime')
for name,sha in json.loads((crt/'manifest.json').read_text())['files'].items():
    assert hashlib.sha256((crt/name).read_bytes()).hexdigest()==sha,name
    for destination in [stage,resources,payload/'cpython']:
        shutil.copy2(crt/name,destination/name)

for name in ['README.md','THIRD_PARTY_NOTICES.md']:shutil.copy2(root/'desktop'/name,stage/name)
shutil.copy2(root/'LICENSE',stage/'LICENSE')
provenance={'schema_version':1,'target':'x86_64-pc-windows-msvc','channel':'internal-development','signed':False,'upstream_commit':'e9b761c08accec7f2ca2a7202a5d710b72faf325','source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'source_diff_sha256':hashlib.sha256(subprocess.check_output(['git','diff','HEAD','--binary'],cwd=root)).hexdigest(),'sidecar_sha256':hashlib.sha256(agent.read_bytes()).hexdigest()}
source_paths=subprocess.check_output(['git','ls-files','-z','--cached','--others','--exclude-standard'],cwd=root).decode().split('\0')
source_hashes={name:hashlib.sha256((root/name).read_bytes()).hexdigest() for name in sorted(set(source_paths)) if name and (root/name).is_file()}
(resources/'source-manifest.json').write_text(json.dumps(source_hashes,indent=2,ensure_ascii=False)+'\n')
provenance['source_manifest_sha256']=hashlib.sha256((resources/'source-manifest.json').read_bytes()).hexdigest()
provenance['desktop_sha256']=hashlib.sha256(exe.read_bytes()).hexdigest()
(resources/'build-provenance.json').write_text(json.dumps(provenance,indent=2)+'\n')
output=artifacts/f'Codewhale-MedSci_{args.version}_windows-x64_internal-setup.exe'
def q(path):return str(path).replace('$','$$').replace('"','$\\"')
# Uninstall only the files this installer owns, leaving any user-created files.
deletes=[];dirs=[]
for path in sorted(stage.rglob('*'),reverse=True):
 relative=str(path.relative_to(stage)).replace('/','\\')
 if path.is_file():deletes.append('  Delete "$INSTDIR\\'+q(relative)+'"')
 elif path.is_dir():dirs.append('  RMDir "$INSTDIR\\'+q(relative)+'"')
script=f'''Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "{q(root/'desktop/packaging/windows-hooks.nsh')}"
!include "{q(root/'desktop/packaging/windows-webview2.nsh')}"
Name "Codewhale-MedSci Internal Preview"
OutFile "{q(output)}"
InstallDir "$LOCALAPPDATA\\Programs\\Codewhale-MedSci"
RequestExecutionLevel user
SetCompressor /SOLID lzma
Icon "{q(root/'apps/desktop/src-tauri/icons/icon.ico')}"
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"
Section "Install"
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"
  File /oname=WebView2Setup.exe "{q(root/'desktop/packaging/MicrosoftEdgeWebview2Setup.exe')}"
  !insertmacro MEDSCI_ENSURE_WEBVIEW2 "$PLUGINSDIR\\WebView2Setup.exe"
  ${{If}} $WebView2Ready == 0
    MessageBox MB_ICONSTOP "未检测到可用的 Microsoft WebView2 Runtime。请检查网络或公司安装策略，安装微软官方 WebView2 Runtime 后重试。返回码：$WebView2ExitCode" /SD IDOK
    SetErrorLevel 2
    Abort
  ${{EndIf}}
  SetOutPath "$INSTDIR"
  File /r "{q(stage/'*')}"
  WriteUninstaller "$INSTDIR\\Uninstall.exe"
  CreateShortcut "$SMPROGRAMS\\Codewhale-MedSci.lnk" "$INSTDIR\\medsci-desktop.exe"
  !insertmacro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\CodewhaleMedSci" "DisplayName" "Codewhale-MedSci Internal Preview"
  WriteRegStr HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\CodewhaleMedSci" "DisplayVersion" "{args.version}"
  WriteRegStr HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\CodewhaleMedSci" "UninstallString" '$\\"$INSTDIR\\Uninstall.exe$\\"'
  WriteRegDWORD HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\CodewhaleMedSci" "NoModify" 1
  WriteRegDWORD HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\CodewhaleMedSci" "NoRepair" 1
SectionEnd
Section "Uninstall"
  !insertmacro NSIS_HOOK_PREUNINSTALL
  DeleteRegKey HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\CodewhaleMedSci"
  Delete "$SMPROGRAMS\\Codewhale-MedSci.lnk"
{chr(10).join(deletes)}
{chr(10).join(dirs)}
  Delete "$INSTDIR\\Uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
'''
nsi=artifacts/f'windows-{args.version}.nsi';nsi.write_text(script,encoding='utf-8')
flag='/' if os.name=='nt' else '-'
subprocess.run([args.makensis,flag+'INPUTCHARSET','UTF8',flag+'V2',str(nsi)],check=True)
output.with_suffix('.exe.sha256').write_text(hashlib.sha256(output.read_bytes()).hexdigest()+'  '+output.name+'\n')
print(output)
