# Skia inside the image

Skia reaches this process through JNI, and a JNI library is normally a file opened by name
at run time. That is one more file every application carries: 21MB beside the renderer on
macOS, and on Windows a 14MB DLL plus a 10MB ICU data file, which is why a single Windows
executable is not possible today.

This is how far linking it in has got, and what is still in the way.

## What works

- **A static Skia exists.** `build-static-skiko.sh` compiles skiko's own C++ and
  Objective-C bindings and archives them together with the Skia libraries skiko's build
  downloads. Nothing in skiko's build is modified: the link task's two inputs are still on
  disk when it finishes, and the archive is made from exactly those.
  About two minutes, and 79MB.
- **The image links it.** `StaticSkikoFeature` registers the name as a library that is
  already inside the image. It reaches three things under `com.oracle.svm.core` to do it,
  none of which is a public API, which is why the build passes `--add-exports` for them and
  why this is off unless `DXC_STATIC_SKIKO` names an archive.
- **The renderer comes out with no undefined Skia symbols.** 70.5MB becomes 83MB.

Three things had to be right and each failed first:

- `-force_load`, because a JNI entry point is reached by name and nothing refers to it by
  symbol, so ordinary archive semantics drop the member that defines it.
- The Objective-C compile task, which is a second task in skiko's build and supplies every
  Metal and AppKit entry point. Without it the archive is missing exactly the platform it
  was built for.
- Stubs for other platforms' entry points. Skiko declares every platform's native methods
  everywhere and compiles only one platform's. That is invisible while the library is
  loaded by name, and fatal once it is linked in, because macOS binds every symbol at load
  and the first Direct3D declaration kills the process before anything is drawn.
  `generate-foreign-stubs.sh` writes the 62 of them.

## What is in the way

Skiko's desktop loader does not use `System.loadLibrary`. It reads `skiko.library.path` and
calls `System.load` with an absolute path, so it fails on the missing file however the
image is linked. Android is the one platform where it calls `System.loadLibrary`, and that
is the call a built-in library replaces.

So the remaining step is to stop that loader from running, which means substituting it in
the image. Substitutions live under `com.oracle.svm.core.annotate`, which is another
internal interface, so the honest options are:

- substitute `LibraryLoader` in the image, accepting a second unstable dependency, or
- ask skiko for a way to say the library is already present, which is the same shape as the
  Android branch it already has.

Nothing here is on by default. An ordinary build loads Skia from the file beside it exactly
as before.
