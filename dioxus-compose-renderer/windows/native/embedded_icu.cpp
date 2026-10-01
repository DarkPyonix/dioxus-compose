// ICU's data, inside the executable rather than beside it.
//
// Skia's Windows build looks for icudtl.dat next to the module and gives up without it,
// which would make a 10MB data file part of every application. ICU itself can take its
// data from memory; only Skia's loader insists on a file. This defines the loader Skia
// calls, so the linker never pulls in Skia's own, and hands ICU the bytes compiled in
// below. The object has to be linked as an object rather than a library member for that
// to hold: objects are always linked, library members only when something is missing.
#include <cstdint>

extern "C" {
// ICU is built with the _skiko suffix on every entry point.
void udata_setCommonData_skiko(const void *data, int *error);
void udata_setFileAccess_skiko(int access, int *error);
}

// ICU requires its common data to be 16-byte aligned. The file is found through the
// compiler's --embed-dir, which the build points at the Skia distribution.
alignas(16) static const unsigned char icu_data[] = {
#embed "icudtl.dat"
};

// UDATA_ONLY_PACKAGES, the setting Skia's own loader uses: look data up in the package
// handed over here and nowhere else.
static constexpr int ONLY_PACKAGES = 1;

bool SkLoadICU() {
    static const bool loaded = [] {
        int error = 0;  // U_ZERO_ERROR; failures are positive, warnings negative
        udata_setCommonData_skiko(icu_data, &error);
        if (error > 0) return false;
        udata_setFileAccess_skiko(ONLY_PACKAGES, &error);
        return error <= 0;
    }();
    return loaded;
}
