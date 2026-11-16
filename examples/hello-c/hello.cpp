// A mod in C++: writes one line to the log when the loader starts it.

#include "enshwick.h"

namespace {

// constinit: the manifest is in the DLL's data from the start, as a mod built
// without the C++ runtime needs.
constinit const EnshwickManifest MANIFEST = {
    .header = {sizeof(EnshwickManifest), ENSHWICK_ABI_MAJOR, ENSHWICK_ABI_MINOR},
    .id = ENSHWICK_STR_INIT("you.hello-cpp"),
    .version = ENSHWICK_STR_INIT("0.1.0"),
    .side = EnshwickSide_SERVER,
    .flags = EnshwickManifestFlags_NONE,
    .name = ENSHWICK_STR_INIT("Hello from C++"),
    .description = ENSHWICK_STR_INIT("Writes one line to the log when it loads."),
    .license = ENSHWICK_STR_INIT("MIT"),
};

} // namespace

uint16_t enshwick_mod_abi(void) { return ENSHWICK_ABI_MAJOR; }

const EnshwickManifest *enshwick_mod_query(void) { return &MANIFEST; }

EnshwickStatus enshwick_mod_init(const EnshwickHostApi *host, EnshwickModHandle me) {
    const EnshwickStr line = ENSHWICK_STR_INIT("hello from C++");
    host->log(me, EnshwickLevel_INFO, line);
    return EnshwickStatus_OK;
}

EnshwickStatus enshwick_mod_ready(const EnshwickHostApi *, EnshwickModHandle) { return EnshwickStatus_OK; }
