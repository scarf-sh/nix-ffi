#include <config.h>
#include <command.hh>
#include "testsuite.hh"

using namespace nix;

struct CmdFci : NixMultiCommand
{
    CmdFci() : MultiCommand({
            { "testsuite", [](){ return make_ref<CmdTestsuite>(); } }
        })
    {
    }

    std::string description() override
    {
        return "programmatic access to Nix primitives";
    }

    void run() override
    {
        if (!command)
            throw UsageError("no fci subcommand specified");
        command->second->prepare();
        command->second->run();
    }
};

static auto r1 = registerCommand<CmdFci>("fci");
