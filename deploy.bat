@echo off
setlocal enabledelayedexpansion
chcp 65001 >nul
title PoxiPass - generar distribuibles
cd /d "%~dp0"

echo.
echo  ==========================================================
echo    PoxiPass  -  instalador (auto-update) + portable
echo  ==========================================================
echo.

REM ---------------------------------------------------------------- requisitos
where node >nul 2>&1
if errorlevel 1 (
  echo  [X] No se encuentra Node.js en el PATH.
  goto :fin_error
)
where cargo >nul 2>&1
if errorlevel 1 (
  echo  [X] No se encuentra Rust/cargo en el PATH.
  goto :fin_error
)
if not exist ".env" (
  echo  [X] Falta el archivo .env con la clave que firma las actualizaciones.
  echo      Sin ella no se puede generar el auto-update.
  goto :fin_error
)

REM La clave privada de firma (TAURI_SIGNING_PRIVATE_KEY y su contrasena) sale del .env
for /f "usebackq eol=# tokens=1,* delims==" %%a in (".env") do set "%%a=%%b"

set "CLI=node_modules\.bin\tauri.cmd"
if not exist "%CLI%" (
  echo  [1/3] Instalando dependencias de Node...
  call npm install
  if errorlevel 1 goto :fin_error
  echo.
)

for /f "delims=" %%v in ('node -p "require('./src-tauri/tauri.conf.json').version"') do set "VER=%%v"

REM ------------------------------------------------------------------ compilar
echo  [2/3] Compilando PoxiPass %VER% en modo release...
echo        La primera vez tarda varios minutos (LTO para que el exe quede pequeno).
echo.
call "%CLI%" build
if errorlevel 1 (
  echo.
  echo  [X] La compilacion ha fallado. Revisa los mensajes de arriba.
  goto :fin_error
)

REM ------------------------------------------------------------------ recoger
set "REL=src-tauri\target\release"
set "NSIS=%REL%\bundle\nsis"
set "OUT=deploy-hosting"
set "EXE=%REL%\PoxiPass.exe"
if not exist "%EXE%" set "EXE=%REL%\poxipass.exe"

if not exist "%EXE%" (
  echo  [X] No se ha generado el ejecutable en %REL%
  goto :fin_error
)
if not exist "%NSIS%\PoxiPass_%VER%_x64-setup.exe.sig" (
  echo  [X] Falta la firma del instalador: revisa la clave del .env
  goto :fin_error
)

echo.
echo  [3/3] Preparando %OUT%...
if exist "%OUT%" rmdir /s /q "%OUT%"
mkdir "%OUT%"

copy /y "%NSIS%\PoxiPass_%VER%_x64-setup.exe" "%OUT%\PoxiPass-Setup-%VER%.exe" >nul || goto :fin_error
copy /y "%NSIS%\PoxiPass_%VER%_x64-setup.exe.sig" "%OUT%\PoxiPass-Setup-%VER%.exe.sig" >nul || goto :fin_error
copy /y "%EXE%" "%OUT%\PoxiPass-Portable-%VER%.exe" >nul || goto :fin_error
call node scripts\latest.mjs "%OUT%"
if errorlevel 1 goto :fin_error

REM Extension del navegador: zip para la release (se carga descomprimida) y zip sin key para las tiendas
call node extension/build.mjs --store
if errorlevel 1 goto :fin_error
move /y "extension\PoxiPass-Extension-%VER%.zip" "%OUT%\" >nul || goto :fin_error
move /y "extension\poxipass-extension-%VER%-tiendas.zip" "%OUT%\" >nul || goto :fin_error

REM --------------------------------------------------------------------- pesos
echo.
echo  ==========================================================
echo    Listo:  %OUT%   (sube los 5 archivos a la release v%VER%;
echo            el zip "-tiendas" va a Chrome Web Store / Edge Add-ons)
echo  ==========================================================
powershell -NoProfile -Command "Get-ChildItem '%OUT%' | ForEach-Object { '   {0,-34} {1,8:N2} MB' -f $_.Name, ($_.Length/1MB) }"
echo.
echo    Referencia: por encima de ~20 MB algo va mal en el bundle.
echo.
pause
endlocal
goto :eof

:fin_error
echo.
pause
endlocal
exit /b 1
