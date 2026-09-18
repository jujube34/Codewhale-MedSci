!include "LogicLib.nsh"
!macro NSIS_HOOK_POSTINSTALL
 ; PortableGit requires its own post-install script after extraction.
 ${If} ${FileExists} "$INSTDIR\resources\git-bash\post-install.bat"
   SetOutPath "$INSTDIR\resources\git-bash"
   DetailPrint "正在初始化内置 Git Bash..."
   ExecWait '$\"$INSTDIR\resources\git-bash\git-bash.exe$\" --no-needs-console --hide --no-cd --command=post-install.bat' $0
   ${If} $0 != 0
     MessageBox MB_ICONSTOP "内置 Git Bash 初始化失败。返回码：$0" /SD IDOK
     SetErrorLevel 2
     Abort
   ${EndIf}
   SetOutPath "$INSTDIR"
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
