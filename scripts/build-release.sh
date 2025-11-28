#!/bin/bash
# Build script for CP Editor release builds
# Usage: ./scripts/build-release.sh [platform]
#   platform: linux, macos, windows, or all (default: current platform)

set -e

PROJECT_NAME="cp-editor"
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*= *"\(.*\)"/\1/')
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
DIST_DIR="$PROJECT_DIR/dist"

echo "Building CP Editor v$VERSION"

# Detect current platform
detect_platform() {
    case "$(uname -s)" in
        Linux*)  echo "linux";;
        Darwin*) echo "macos";;
        MINGW*|CYGWIN*|MSYS*) echo "windows";;
        *)       echo "unknown";;
    esac
}

CURRENT_PLATFORM=$(detect_platform)
TARGET_PLATFORM="${1:-$CURRENT_PLATFORM}"

# Create dist directory
mkdir -p "$DIST_DIR"

build_linux() {
    echo "Building for Linux..."

    # Build release binary
    cargo build --release -p cp-editor

    # Create distribution package
    LINUX_DIST="$DIST_DIR/cp-editor-$VERSION-linux-x86_64"
    mkdir -p "$LINUX_DIST"

    # Copy binary
    cp "$PROJECT_DIR/target/release/cp-editor" "$LINUX_DIST/"

    # Create desktop file
    cat > "$LINUX_DIST/cp-editor.desktop" << EOF
[Desktop Entry]
Name=CP Editor
Comment=GPU-accelerated text editor
Exec=cp-editor %F
Icon=cp-editor
Type=Application
Categories=Development;TextEditor;
MimeType=text/plain;text/x-csrc;text/x-c++src;text/x-python;text/x-rust;application/json;
Terminal=false
EOF

    # Create tarball
    cd "$DIST_DIR"
    tar -czvf "cp-editor-$VERSION-linux-x86_64.tar.gz" "cp-editor-$VERSION-linux-x86_64"

    echo "Linux build complete: $DIST_DIR/cp-editor-$VERSION-linux-x86_64.tar.gz"
}

build_macos() {
    echo "Building for macOS..."

    # Build release binary
    cargo build --release -p cp-editor

    # Create app bundle
    APP_DIR="$DIST_DIR/CP Editor.app"
    CONTENTS_DIR="$APP_DIR/Contents"
    MACOS_DIR="$CONTENTS_DIR/MacOS"
    RESOURCES_DIR="$CONTENTS_DIR/Resources"

    mkdir -p "$MACOS_DIR"
    mkdir -p "$RESOURCES_DIR"

    # Copy binary
    cp "$PROJECT_DIR/target/release/cp-editor" "$MACOS_DIR/"

    # Create Info.plist
    cat > "$CONTENTS_DIR/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>cp-editor</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>com.yigit.cp-editor</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>CP Editor</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.13</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>
            <string>Text Document</string>
            <key>CFBundleTypeRole</key>
            <string>Editor</string>
            <key>LSItemContentTypes</key>
            <array>
                <string>public.plain-text</string>
                <string>public.source-code</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
EOF

    # Create DMG (if hdiutil is available)
    if command -v hdiutil &> /dev/null; then
        DMG_NAME="cp-editor-$VERSION-macos.dmg"
        hdiutil create -volname "CP Editor" -srcfolder "$APP_DIR" -ov -format UDZO "$DIST_DIR/$DMG_NAME" || true
        echo "macOS build complete: $DIST_DIR/$DMG_NAME"
    else
        echo "macOS build complete: $APP_DIR"
    fi
}

build_windows() {
    echo "Building for Windows..."

    # Build release binary
    cargo build --release -p cp-editor

    # Create distribution package
    WIN_DIST="$DIST_DIR/cp-editor-$VERSION-windows-x86_64"
    mkdir -p "$WIN_DIST"

    # Copy binary
    if [ -f "$PROJECT_DIR/target/release/cp-editor.exe" ]; then
        cp "$PROJECT_DIR/target/release/cp-editor.exe" "$WIN_DIST/"
    else
        cp "$PROJECT_DIR/target/release/cp-editor" "$WIN_DIST/cp-editor.exe"
    fi

    # Create zip
    cd "$DIST_DIR"
    if command -v zip &> /dev/null; then
        zip -r "cp-editor-$VERSION-windows-x86_64.zip" "cp-editor-$VERSION-windows-x86_64"
        echo "Windows build complete: $DIST_DIR/cp-editor-$VERSION-windows-x86_64.zip"
    else
        echo "Windows build complete: $WIN_DIST"
    fi
}

# Build for specified platform
case "$TARGET_PLATFORM" in
    linux)
        build_linux
        ;;
    macos)
        build_macos
        ;;
    windows)
        build_windows
        ;;
    all)
        echo "Building for all platforms (current platform: $CURRENT_PLATFORM)"
        build_"$CURRENT_PLATFORM"
        echo ""
        echo "Note: Cross-compilation requires additional setup."
        echo "Build on each target platform for best results."
        ;;
    *)
        echo "Unknown platform: $TARGET_PLATFORM"
        echo "Supported platforms: linux, macos, windows, all"
        exit 1
        ;;
esac

echo ""
echo "Build artifacts are in: $DIST_DIR"
