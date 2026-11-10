# Sessions

Mush is a shell over one workspace. The native `mush` process is the host: it
opens the workspace, links the grants and calls the Spore one line at a time.
The Spore owns parsing, `PWD`/`OLDPWD`, environment variables, startup files and
command semantics.

`mush --root DIR` starts a session in DIR. Without `--root` the workspace is
`~/.local/share/fungos/vfs`. `mush --script FILE` runs commands from a file and
exits.

# Commands

Built in: `help`, `pwd`, `cd`, `ls`, `mkdir`, `rmdir`, `touch`, `cat`, `write`,
`append`, `head`, `tail`, `grep`, `wc`, `stat`, `tree`, `find`, `rm`, `mv`, `cp`,
`search`, `source`, `echo`, `export`, `unset`, `env`, `set`, `exit`.

`CMD > PATH` and `CMD >> PATH` write through the versioned store, so a redirect
creates a new version like any other write.

# Installed commands and Spores

Unknown names resolve through the virtual `PATH`: installed wasm commands, and
installed Spores for identity reads. `commands` lists the first set and `which`
answers for one name. `man NAME` prints the manual of an installed Spore, and
`NAME --version` prints its identity line. Nothing is executed from the host
`PATH`.

# What it cannot do

No host filesystem, no host binary lookup, no subprocess, no network. `.mushrc`
is workspace content, not a host dotfile. A script is a list of Mush commands:
there is no `/bin/sh` and no `exec` behind it.
