#pragma once

#include <command.hh>

using namespace nix;

struct CmdTestsuite : NixMultiCommand
{
    CmdTestsuite();
    std::string description() override;
    void run() override;
};
