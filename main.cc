#include <config.h>
#include <command.hh>

using namespace nix;

struct CmdFfid : Command
{
    void run() override
    {
    }

    std::string description() override
    {
        return "the Nix FFI daemon";
    }
};

static auto r1 = registerCommand<CmdFfid>("ffid");
