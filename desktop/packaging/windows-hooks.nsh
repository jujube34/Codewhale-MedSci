!include "LogicLib.nsh"
!define MEDSCI_GIT_INIT_SCRIPT "${__FILEDIR__}\windows-git-init.cmd"
!macro NSIS_HOOK_POSTINSTALL
 ; Use an absolute command, scoped environment, and verified completion.
 InitPluginsDir
 SetOutPath "$PLUGINSDIR"
 File /oname=medsci-git-init.cmd "${MEDSCI_GIT_INIT_SCRIPT}"
 SetOutPath "$INSTDIR"
 DetailPrint "正在初始化内置 Git Bash..."
 nsExec::ExecToLog /TIMEOUT=60000 '$\"$SYSDIR\cmd.exe$\" /D /S /C $\"$\"$PLUGINSDIR\medsci-git-init.cmd$\" $\"$INSTDIR\resources\git-bash$\"$\"'
 Pop $0
 ${If} $0 != 0
   MessageBox MB_ICONSTOP "内置 Git Bash 初始化或自检失败。请查看安装详细日志。返回码：$0" /SD IDOK
   SetErrorLevel 2
   Abort
 ${EndIf}
 SetShellVarContext current
 WriteRegStr HKCU "Software\Classes\Directory\shell\CodewhaleMedSci" "Icon" '$\"$INSTDIR\medsci-desktop.exe$\",0'
 WriteRegStr HKCU "Software\Classes\Directory\shell\CodewhaleMedSci" "MultiSelectModel" "Single"
 WriteRegStr HKCU "Software\Classes\Directory\Background\shell\CodewhaleMedSci" "Icon" '$\"$INSTDIR\medsci-desktop.exe$\",0'
 WriteRegStr HKCU "Software\Classes\Directory\shell\CodewhaleMedSci" "" "在 Codewhale-MedSci 中打开"
 WriteRegStr HKCU "Software\Classes\Directory\shell\CodewhaleMedSci\command" "" '$\"$INSTDIR\medsci-desktop.exe$\" $\"%1\.$\"'
 WriteRegStr HKCU "Software\Classes\Directory\Background\shell\CodewhaleMedSci" "" "在 Codewhale-MedSci 中打开"
 WriteRegStr HKCU "Software\Classes\Directory\Background\shell\CodewhaleMedSci\command" "" '$\"$INSTDIR\medsci-desktop.exe$\" $\"%V\.$\"'
 System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend
!macro NSIS_HOOK_PREUNINSTALL
 ; Includes runtime-generated post-install links, never user workspaces.
 RMDir /r "$INSTDIR\resources\git-bash"
 DeleteRegKey HKCU "Software\Classes\Directory\shell\CodewhaleMedSci"
 DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\CodewhaleMedSci"
 System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend
