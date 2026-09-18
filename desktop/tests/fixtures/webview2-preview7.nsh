  ExecWait '$\"$PLUGINSDIR\WebView2Setup.exe$\" /silent /install' $0
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "Microsoft WebView2 installation failed. Install the official WebView2 Runtime, then retry. Code: $0"
    Abort
  ${EndIf}
