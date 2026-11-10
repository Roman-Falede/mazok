@echo off
chcp 65001 >nul
rem Открывает Мазок отдельным окном приложения в Microsoft Edge (без адресной строки)
set "APP=%~dp0index.html"
set "URL=file:///%APP:\=/%"
start "" msedge --app="%URL%" --window-size=1400,900
