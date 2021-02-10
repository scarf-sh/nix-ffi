#include <config.h>
#include <command.hh>

#define ADD_TEMPROOT 0

using namespace nix;

const static char ok = '\0';

class CmdFfiHelper : public StoreCommand
{
    FdSource in = STDIN_FILENO;
    FdSink out = STDOUT_FILENO;

    void run(ref<Store> store) override
    {
        vector<char> buf(4096);
        while (true) {
            char command;
            try {
                in(&command, sizeof command);
            } catch (EndOfFile &){
                break;
            }
            if (command == ADD_TEMPROOT) {
                size_t len;
                in((char *) &len, sizeof len);
                if (len > buf.size())
                    buf.resize(len);
                in(buf.data(), len);
                store->addTempRoot(StorePath(std::string_view(buf.data(), len)));
                out(std::string_view(&ok, sizeof ok));
            } else {
                throw Error("unknown command byte %c%", command);
            }
        }
    }

    std::string description() override
    {
        return "the Nix FFI helper";
    }
};

static auto r1 = registerCommand<CmdFfiHelper>("ffi-helper");
