# What Compose has to be patched to do, and why it is here rather than in a fork

The renderer draws with Compose Multiplatform. Three things it needs are not in the
published build, and none of them can be supplied from outside the module that holds
them: they are `internal actual` declarations, and an `expect` can only be answered
inside its own module. A published artifact cannot be extended into them and a
compile-time flag cannot reach them.

So the source is changed. It is kept as patches against a pinned upstream revision
rather than as a fork, because a fork is a second repository to keep alive and these
are about two hundred lines that want reviewing next to the code that depends on them.
`scripts/build-compose.sh` fetches that revision into a work directory, applies these
in order, and publishes what it builds to the local Maven repository under the version
the renderer asks for.

| Patch | What it answers |
|---|---|
| `0001-linux-native-targets.patch` | Compose publishes no Kotlin/Native target for Linux at all |
| `0002-native-text-context-menu.patch` | The context menu is an empty function upstream, the menu entries were never built, and copying with the keyboard threw |
| `0003-publish-as-1.11.1.patch` | What the patched build publishes as |

## The pin

    https://github.com/JetBrains/compose-multiplatform-core.git
    73ac84978a9e4ddca7e062dc0ee357ad875450fa   (release/1.11)

Pinned to a revision rather than to a branch, because a patch that applies today and
not tomorrow is a build that breaks for a reason nobody changed.

## When upstream moves

Move the pin, run the script, and read what refuses to apply. A patch that no longer
applies because upstream has filled the same gap is a patch to delete, and that is the
outcome to hope for: every one of these is a hole in the platform rather than something
this project wanted differently.

## skiko

skiko publishes no Kotlin/Native target for Windows, so the Windows renderer cannot be
built from what JetBrains publishes. These patches are applied to skiko the same way the
ones above are applied to Compose, by `scripts/build-skiko-windows.sh`.

| Patch | What it answers |
|---|---|
| `skiko/0001-mingw-x64-target.patch` | A `mingwX64` target that builds the Kotlin half only; the C++ half is compiled with MSVC by the script, because it has to match the ABI of the prebuilt Windows Skia |

### The pin

    https://github.com/JetBrains/skiko.git
    9a5b398bb2044fff7e7a84fbfd6f4b803e4427c0   (v0.144.6)

The version Compose 1.11 asks for. Skia is the one that skiko revision names,
`m144-22f58c9fd4`, from JetBrains' own Skia builds.
