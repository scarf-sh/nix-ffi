#pragma once

#include <command.hh>

using namespace nix;

class CmdTestsuite : public NixMultiCommand
{
    std::optional<Path> testRoot;
    std::string description() override;
    void run() override;
public:
    CmdTestsuite();
};
