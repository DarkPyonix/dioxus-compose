# Shared settings for the native build scripts. Sourced, not executed.

NATIVE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_DIR="$(cd "$NATIVE_DIR/.." && pwd)"
BUILD_DIR="$PROJECT_DIR/build/native-image"
DIST_DIR="$BUILD_DIR/dist"
LIBRARY_NAME="libdioxus_compose_renderer"
METADATA_DIR="$NATIVE_DIR/resources/META-INF/native-image/org.thisisthepy/dioxus-compose-renderer"

# macOS needs Liberica NIK Full: upstream GraalVM skips AWT on Darwin (oracle/graal#13272).
if [[ -z "${GRAALVM_HOME:-}" ]]; then
    for candidate in "$HOME"/Library/Java/JavaVirtualMachines/bellsoft-liberica-vm-full-openjdk25*/Contents/Home; do
        [[ -x "$candidate/bin/native-image" ]] && GRAALVM_HOME="$candidate"
    done
fi
if [[ -z "${GRAALVM_HOME:-}" || ! -x "$GRAALVM_HOME/bin/native-image" ]]; then
    echo "error: set GRAALVM_HOME to a Liberica NIK 25 Full installation" >&2
    exit 1
fi

CLASSPATH_FILE="$BUILD_DIR/classpath.txt"

# Runs the native module on the JVM through the Kotlin Toolchain, which resolves the runtime
# classpath correctly. (Its executable jar does not: JetBrains and Google both publish a
# `lifecycle-common-jvm-2.11.0.jar`, and the second overwrites the first inside the jar.)
#
# The JVM prints its properties at startup; java.class.path is captured from that output
# into $CLASSPATH_FILE so native-image builds against exactly what ran.
run_on_jvm() {
    local extra_jvm_args="$1"
    local log="$BUILD_DIR/jvm-run.log"
    mkdir -p "$BUILD_DIR"
    # Metadata describes the JDK it was collected on, so the JVM run uses NIK itself.
    (cd "$PROJECT_DIR" && JAVA_HOME="$GRAALVM_HOME" ./kotlin run -m native --no-compose-hot-reload \
        --jvm-args="-XshowSettings:properties $extra_jvm_args") 2>&1 | tee "$log" >&2
    awk '
        /^ *java\.class\.path = / { sub(/^ *java\.class\.path = /, ""); print; collecting = 1; next }
        collecting && /^ {8,}[^ ]/ { sub(/^ +/, ""); print; next }
        collecting { exit }
    ' "$log" | paste -sd: - > "$CLASSPATH_FILE"
    [[ -s "$CLASSPATH_FILE" ]] || { echo "error: could not read java.class.path from $log" >&2; exit 1; }
}
