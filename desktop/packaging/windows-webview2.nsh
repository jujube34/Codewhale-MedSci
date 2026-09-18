!ifndef MEDSCI_WEBVIEW2_INCLUDED
!define MEDSCI_WEBVIEW2_INCLUDED
!include "LogicLib.nsh"
!include "WordFunc.nsh"
!define MEDSCI_WEBVIEW2_KEY "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
Var WebView2Ready
Var WebView2Version
Var WebView2ExitCode

; Microsoft documents the 32-bit HKLM view and HKCU pv value for Evergreen.
; Restore the caller's registry view and registers after detection.
Function DetectWebView2
  Push $0
  Push $1
  StrCpy $WebView2Ready 0
  StrCpy $WebView2Version ""
  SetRegView 32
  ReadRegStr $0 HKLM "${MEDSCI_WEBVIEW2_KEY}" "pv"
  ${If} $0 != ""
    ${VersionCompare} "$0" "0.0.0.0" $1
    ${If} $1 == 1
      StrCpy $WebView2Ready 1
      StrCpy $WebView2Version $0
    ${EndIf}
  ${EndIf}
  ${If} $WebView2Ready == 0
    ReadRegStr $0 HKCU "${MEDSCI_WEBVIEW2_KEY}" "pv"
    ${If} $0 != ""
      ${VersionCompare} "$0" "0.0.0.0" $1
      ${If} $1 == 1
        StrCpy $WebView2Ready 1
        StrCpy $WebView2Version $0
      ${EndIf}
    ${EndIf}
  ${EndIf}
  SetRegView lastused
  Pop $1
  Pop $0
FunctionEnd

; Caller supplies the extracted bootstrapper. Runtime presence, not merely
; a nonzero bootstrapper exit code, determines whether the prerequisite is met.
!macro MEDSCI_ENSURE_WEBVIEW2 SETUP
  Call DetectWebView2
  StrCpy $WebView2ExitCode "not-run"
  ${If} $WebView2Ready == 0
    DetailPrint "正在安装 Microsoft WebView2（可能需要联网）..."
    ClearErrors
    ExecWait '"${SETUP}" /silent /install' $WebView2ExitCode
    ${If} ${Errors}
      StrCpy $WebView2ExitCode "无法启动安装程序"
    ${EndIf}
    Call DetectWebView2
  ${EndIf}
  ${If} $WebView2Ready == 1
    DetailPrint "已检测到 WebView2 $WebView2Version，继续安装。"
  ${EndIf}
!macroend
!endif
