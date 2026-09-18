"""Replay Git initialization through the production NSIS hook in isolated Wine.

The batch runs in cmd; small native fixtures stand in for Git/MSYS executables.
This covers installer control flow, not native Windows MSYS compatibility.
An optional hook path allows replaying the same cases against an old revision.
"""
import os
import pathlib
import shutil
import subprocess
import sys

root = pathlib.Path(__file__).resolve().parents[2]
folder = root / 'desktop/artifacts/git-init-test'
folder.mkdir(parents=True, exist_ok=True)
hook = pathlib.Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else root / 'desktop/packaging/windows-hooks.nsh'
env = {k: v for k, v in os.environ.items() if k in ['PATH', 'HOME', 'TMPDIR', 'USER', 'LOGNAME']}
env.update(WINEPREFIX=str(folder / 'wine'), WINEDEBUG='-all')

def win(path):
    return 'Z:' + str(path).replace('/', '\\')

def build(name, body, include=''):
    exe = folder / (name + '.exe')
    source = folder / (name + '.nsi')
    source.write_text(f'Unicode true\nRequestExecutionLevel user\nSilentInstall silent\nOutFile "{exe}"\n!include "LogicLib.nsh"\n{include}\nSection\n{body}\nSectionEnd\n')
    subprocess.run(['makensis', '-V2', str(source)], check=True)
    return exe

# The legacy hook launches this executable and trusts its nonzero exit code.
launcher = build('launcher', '''ExecWait '\"$SYSDIR\\cmd.exe\" /D /C post-install.bat' $0
SetErrorLevel $0''')
# Fail if the helper does not supply its actual Bash self-check command.
bash = build('bash', '''${GetParameters} $0
${StrStr} $1 $0 "MEDSCI_BASH_OK"
${If} $1 == ""
SetErrorLevel 9
Quit
${EndIf}
IfFileExists "$EXEDIR\\probe-fails" 0 +3
SetErrorLevel 7
Quit
SetErrorLevel 0''', '!include "FileFunc.nsh"\n!include "StrFunc.nsh"\n${StrStr}')

cases = [
    ('self-delete-exit-1', 1, True, True, False, 0),
    ('self-delete-exit-0', 0, True, True, False, 0),
    ('batch-remains', 0, False, True, False, 2),
    ('post-step-fails', 1, True, False, False, 2),
    ('bash-fails', 1, True, True, True, 2),
    ('already-initialized', None, True, True, False, 0),
    ('missing-bash', 1, True, True, False, 2),
    ('missing-git', 1, True, True, False, 2),
]
for name, code, delete_batch, delete_posts, bad_probe, expected in cases:
    install = folder / ('公司电脑 with spaces ' + name)
    if install.exists():
        shutil.rmtree(install)
    git = install / 'resources/git-bash'
    (git / 'usr/bin').mkdir(parents=True)
    (git / 'mingw64/bin').mkdir(parents=True)
    shutil.copy2(launcher, git / 'git-bash.exe')
    if name != 'missing-bash':
        shutil.copy2(bash, git / 'usr/bin/bash.exe')
    if name != 'missing-git':
        shutil.copy2(bash, git / 'mingw64/bin/git.exe')
    if bad_probe:
        (git / 'usr/bin/probe-fails').touch()
    if code is not None:
        (git / 'etc/post-install').mkdir(parents=True)
        # Both launch paths must reach the same batch completion in the Git
        # directory, then report the captured self-delete exit code.
        batch = '@echo off\r\nchcp 65001 >nul\r\ncd > cwd.txt\r\necho %PATH% > path.txt\r\n'
        if delete_posts:
            batch += 'rmdir etc\\post-install\r\n'
        batch += '(\r\n'
        if delete_batch:
            batch += 'del post-install.bat\r\n'
        batch += f'exit /b {code}\r\n)\r\n'
        (git / 'post-install.bat').write_bytes(batch.encode('utf-8'))
    exe = build(name, f'''StrCpy $INSTDIR "{win(install)}"
SetOutPath "$TEMP"
!insertmacro NSIS_HOOK_POSTINSTALL
DeleteRegKey HKCU "Software\\Classes\\Directory\\shell\\CodewhaleMedSci"
DeleteRegKey HKCU "Software\\Classes\\Directory\\Background\\shell\\CodewhaleMedSci"
SetErrorLevel 0''', f'!include "{hook}"')
    result = subprocess.run(['wine', str(exe), '/S'], env=env, capture_output=True, timeout=60)
    assert result.returncode == expected, (name, result.returncode, expected, result.stderr.decode(errors='replace'))
    if code is not None and expected == 0:
        assert not (git / 'post-install.bat').exists(), name
        assert (git / 'cwd.txt').read_text().strip().lower() == win(git).lower(), name
        assert (git / 'path.txt').read_text().strip().lower().startswith(win(git).lower() + '\\mingw64\\bin;'), name
    print('PASS: ' + name, flush=True)
print('8 checks passed; 0 failed. Production NSIS and cmd; simulated Git/MSYS binaries.')
