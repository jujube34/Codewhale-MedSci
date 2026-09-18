@echo off
setlocal EnableExtensions DisableDelayedExpansion
rem Resolve both the working directory and tools from the bundled runtime.
cd /d "%~1"
if errorlevel 1 exit /b 1
set "PATH=%~1\mingw64\bin;%~1\usr\bin;%PATH%"
set "MSYSTEM=MINGW64"
if not exist "usr\bin\bash.exe" exit /b 1
if not exist "mingw64\bin\git.exe" exit /b 1
rem PortableGit deletes this batch even when a .post step fails. Its exit
rem code is not a success signal; cmd can also return 1 after self-deletion.
if exist "post-install.bat" call post-install.bat
if exist "post-install.bat" (
  echo Git initialization incomplete: post-install.bat remains.
  exit /b 1
)
if exist "etc\post-install" (
  echo Git initialization incomplete: post-install scripts remain.
  exit /b 1
)
"%~1\usr\bin\bash.exe" --noprofile --norc -c "test -n \"$BASH_VERSION\" && test -e /etc/mtab && pwd >/dev/null && /mingw64/bin/git --version && printf MEDSCI_BASH_OK"
if errorlevel 1 exit /b 1
exit /b 0
