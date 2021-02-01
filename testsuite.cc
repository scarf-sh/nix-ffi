#include <config.h>
#include <progress-bar.hh>
#include <affinity.hh>
#include "testsuite.hh"

class TestsuiteCommand : public virtual Command
{
protected:
    const Path testRoot;
    TestsuiteCommand(Path testRoot) : testRoot(std::move(testRoot)) {}
};

class CmdRun : public TestsuiteCommand
{
    std::vector<std::string> command;

    // Pointers invalidated if command is changed
    std::vector<char *> commandCharPtrs()
    {
        std::vector<char *> res;
        for (auto & s : command) res.push_back((char *) s.c_str());
        res.push_back(nullptr);
        return res;
    }

    std::string description() override
    {
        return "run a program in an environment where Nix operations point to a test store";
    }

    void run() override
    {
        if (command.empty())
            throw UsageError("required argument --command not given");
        stopProgressBar();
        restoreSignals();
        restoreAffinity();
        setenv("NIX_STORE_DIR", (testRoot + "/store").c_str(), 1);
        setenv("NIX_IGNORE_SYMLINK_STORE", "1", 1);
        setenv("NIX_LOCALSTATE_DIR", (testRoot + "/var").c_str(), 1);
        setenv("NIX_LOG_DIR", (testRoot + "/var/log/nix").c_str(), 1);
        setenv("NIX_STATE_DIR", (testRoot + "/var/nix").c_str(), 1);
        setenv("NIX_CONF_DIR", (testRoot + "/etc").c_str(), 1);
        setenv("NIX_DAEMON_SOCKET_PATH", (testRoot + "/daemon-socket").c_str(), 1);
        unsetenv("NIX_USER_CONF_FILES");
        execvp(command[0].c_str(), commandCharPtrs().data());
        throw SysError("unable to execute '%s'", command[0]);
    }

public:
    CmdRun(Path testRoot) : TestsuiteCommand(std::move(testRoot)) {
        addFlag({
                .longName = "command",
                .description = "The command to run, with all of its arguments",
                .labels = {"command", "args"},
                .handler = {[&](std::vector<std::string> ss) {
                    if (ss.empty()) throw UsageError("--command requires at least one argument");
                    command = std::move(ss);
                }}
            });
    }
};

// TODO Functions from nix testsuite: clearStore, clearProfiles, clearCache, clearCacheCache, start daemon, kill daemon
CmdTestsuite::CmdTestsuite() : MultiCommand({
        { "run", [this](){
            if (!testRoot)
                throw UsageError("required argument --test-root not given");
            return make_ref<CmdRun>(*std::move(testRoot));
        } }
    })
{
    addFlag({
            .longName = "test-root",
            .description = "The directory root for the test store and all nix data/config files",
            .labels = {"directory"},
            .handler = {[&](Path p) {
                if (testRoot)
                    throw UsageError("--test-root specified twice");
                testRoot = std::move(p);
            }}
        });
}

std::string CmdTestsuite::description()
{
    return "commands for test suites needing Nix stores";
}

void CmdTestsuite::run()
{
    if (!command)
        throw UsageError("no fci testsuite subcommand specified");
    command->second->prepare();
    command->second->run();
}
