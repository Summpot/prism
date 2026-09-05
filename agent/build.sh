#!/bin/sh
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

echo "[PrismAgent] Building prism-agent.jar..."
rm -rf build/classes
mkdir -p build/classes

javac -encoding UTF-8 -d build/classes src/prism/agent/*.java

# Create manifest file
cat <<EOF > build/manifest.mf
Manifest-Version: 1.0
Premain-Class: prism.agent.PrismAgent
Can-Redefine-Classes: false
Can-Retransform-Classes: false
EOF

jar cfm prism-agent.jar build/manifest.mf -C build/classes .
echo "[PrismAgent] Successfully built prism-agent.jar at $SCRIPT_DIR/prism-agent.jar"
