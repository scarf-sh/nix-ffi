#include <config.h>
#include <command.hh>

using namespace nix;

struct CmdFci : NixMultiCommand
{
    CmdFci() : MultiCommand({})
    {
    }

    std::string description() override
    {
        return "programmatic access to Nix primitives";
    }

    void run() override
    {
        return;
    }
};

static auto r1 = registerCommand<CmdFci>("fci");
