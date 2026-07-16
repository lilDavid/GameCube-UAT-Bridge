#include "Common/CommonUtils.h"
#include "Common/MemoryCommon.h"
#include "DolphinProcess/DolphinAccessor.h"

extern "C" int Dolphin_getStatus(void) {
    return static_cast<int>(DolphinComm::DolphinAccessor::getStatus());
}

extern "C" void Dolphin_hook(void) {
    DolphinComm::DolphinAccessor::hook();
}

extern "C" void Dolphin_unHook(void) {
    DolphinComm::DolphinAccessor::unHook();
}

extern "C" bool Dolphin_isValidConsoleAddress(u32 address) {
    return DolphinComm::DolphinAccessor::isValidConsoleAddress(address);
}

extern "C" bool Dolphin_readBytes(u32 address, char* buffer, const size_t size) {
    return DolphinComm::DolphinAccessor::readFromRAM(
        Common::dolphinAddrToOffset(address, DolphinComm::DolphinAccessor::isARAMAccessible()),
        buffer,
        size,
        false
    );
}
