@echo off
setlocal enabledelayedexpansion

cd /d "%~dp0"

echo [PrismAgent] Building prism-agent.jar...
if exist "build" rmdir /s /q "build"
mkdir "build\classes"

javac -encoding UTF-8 -d build\classes src\prism\agent\*.java
if %errorlevel% neq 0 (
    echo [PrismAgent] Compilation failed.
    exit /b %errorlevel%
)

(
echo Manifest-Version: 1.0
echo Premain-Class: prism.agent.PrismAgent
echo Can-Redefine-Classes: false
echo Can-Retransform-Classes: false
) > build\manifest.mf

jar cfm prism-agent.jar build\manifest.mf -C build\classes .
if %errorlevel% neq 0 (
    echo [PrismAgent] Packaging failed.
    exit /b %errorlevel%
)

echo [PrismAgent] Successfully built prism-agent.jar at %CD%\prism-agent.jar
