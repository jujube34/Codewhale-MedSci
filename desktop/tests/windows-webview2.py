"""Exercise the production NSIS WebView2 prerequisite under isolated Wine."""
import os,pathlib,subprocess
root=pathlib.Path(__file__).resolve().parents[2]
folder=root/'desktop/artifacts/webview2-test';folder.mkdir(exist_ok=True)
prefix=root/'desktop/test-wine-webview2'
env={k:v for k,v in os.environ.items() if k in ['PATH','HOME','TMPDIR','USER','LOGNAME']}
env.update(WINEPREFIX=str(prefix),WINEDEBUG='-all')
key=r'Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
def build(name,body,extra=''):
    exe=folder/(name+'.exe');nsi=folder/(name+'.nsi')
    nsi.write_text(f'Unicode true\nRequestExecutionLevel user\nSilentInstall silent\nOutFile "{exe}"\n!include "LogicLib.nsh"\n{extra}\nSection\n{body}\nSectionEnd\n')
    subprocess.run(['makensis','-V2',str(nsi)],check=True)
    return exe
def win(path):return "Z:"+str(path).replace("/", "\\")
def run(exe):
    return subprocess.run(['wine',str(exe),'/S'],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,timeout=60).returncode
# A fixture returns the exact screenshot HRESULT after registering a valid Runtime.
good=build('setup-already-installed',f'SetRegView 32\nWriteRegStr HKCU "{key}" "pv" "146.0.3856.72"\nSetErrorLevel -2147219416')
bad=build('setup-failed','SetErrorLevel 5')
clear=f'SetRegView 32\nDeleteRegKey HKCU "{key}"\nDeleteRegKey HKLM "{key}"\n'
# Replay the old production condition with the modal UI suppressed.
old=(root/'desktop/tests/fixtures/webview2-preview7.nsh').read_text()
old=old.replace('$PLUGINSDIR\\WebView2Setup.exe',win(good))
old='\n'.join(line for line in old.splitlines() if 'MessageBox' not in line)
old=old.replace('  ${If} $0 != 0', '  ${If} $0 != -2147219416\nSetErrorLevel 77\nQuit\n${EndIf}\n  ${If} $0 != 0')
legacy=build('legacy-regression',clear+old+'\nSetErrorLevel 0')
assert run(legacy)==2,'old installer must reproduce the screenshot failure after exact HRESULT'
print('PASS: old production nonzero-exit check reproduces failure for -2147219416',flush=True)
include=f'!include "{root}/desktop/packaging/windows-webview2.nsh"'
cases=[('user-present',f'WriteRegStr HKCU "{key}" "pv" "146.0.3856.72"',bad,1,'not-run'),
       ('machine-present',f'WriteRegStr HKLM "{key}" "pv" "146.0.3856.72"',bad,1,'not-run'),
       ('missing-installed','',good,1,'-2147219416'),
       ('zero-version',f'WriteRegStr HKCU "{key}" "pv" "0.0.0.0"',good,1,'-2147219416'),
       ('empty-version',f'WriteRegStr HKCU "{key}" "pv" ""',good,1,'-2147219416'),
       ('real-failure','',bad,0,'5')]
for name,registry,setup,ready,code in cases:
    body=clear+registry+f'\n!insertmacro MEDSCI_ENSURE_WEBVIEW2 "{win(setup)}"\n'
    body+=f'${{If}} $WebView2Ready != {ready}\nSetErrorLevel 21\nQuit\n${{EndIf}}\n'
    body+=f'${{If}} $WebView2ExitCode != "{code}"\nSetErrorLevel 22\nQuit\n${{EndIf}}\nSetErrorLevel 0'
    result=run(build(name,body,include));assert result==0,(name,result)
    print('PASS: '+name,flush=True)
assert run(build('cleanup',clear+'SetErrorLevel 0'))==0
print('7 checks passed; 0 failed. Fixtures test NSIS control flow and registry detection, not real WebView2 installation.')
