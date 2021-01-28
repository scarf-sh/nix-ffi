#include <config.h>
#include "testsuite.hh"

CmdTestsuite::CmdTestsuite() : MultiCommand({})
{
}

std::string CmdTestsuite::description()
{
    return "commands for test suites needing Nix stores";
}

void CmdTestsuite::run()
{
    return;
}
