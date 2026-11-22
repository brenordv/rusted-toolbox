# Profiles: a quick guide

A profile is a connection you save once, under a name, and use from any folder, in `eh` and in
eh-gui. It keeps the event hub (namespace, event hub, consumer group, how you sign in), the
settings you saved with it, and its own data folder, so each profile has its own archive,
checkpoints, and exports.

Profiles live in the workspace: `.eh/profiles` in your home folder, one folder per profile. The
`eh` readme's "Profiles" section has the full rules; this page shows the common tasks.

## Save your first profile

With a connection string, from any folder:

```bash
export EH_CONNECTION_STRING='Endpoint=sb://my-ns.servicebus.windows.net/;SharedAccessKeyName=listen;SharedAccessKey=...;EntityPath=orders'
eh read --profile orders-dev --save-config
# Saved profile orders-dev; the connection string is stored in .../orders-dev/.eh-data/secrets.redb (not encrypted)
# It then starts reading. Press Ctrl-C once it says "Reading partitions".
unset EH_CONNECTION_STRING
```

The variable keeps the string out of your shell history; `-s '<string>'` works too. The profile
is saved before `eh` connects, so a wrong string is saved as well: run the same command again
with the right one.

With Entra ID, nothing secret is stored at all:

```bash
az login
eh read --profile orders-prod --save-config --auth developer --namespace orders-prod.servicebus.windows.net -e orders
```

## Use it from any folder

```bash
eh read --profile orders-dev              # capture into the profile's archive
eh export --profile orders-dev -f json    # export from that archive
eh send --profile orders-dev --body ping  # send to the profile's event hub
eh profile list                           # every profile, its event hub and how it signs in
```

`eh send` doesn't ask you to confirm the destination for a profile that names its namespace and
event hub (one saved from a connection string with an `EntityPath`, or with `--namespace` and
`-e`). Give production profiles names you won't pick by accident, like `orders-prod`.

## Change a saved setting

Run with the flag and `--save-config` to keep it:

```bash
eh read --profile orders-dev -g eh-local --save-config   # from now on, consumer group eh-local
```

Without `--save-config`, a flag changes only that run:

```bash
eh read --profile orders-dev -p 0   # partition 0, this time only
```

## Pick from a menu

Run `eh` with no arguments in a terminal. It lists your profiles, asks what to do, and prints the
command it is about to run:

```
> Profile: orders-dev  namespace orders.servicebus.windows.net, event hub orders
> Action: read
> Consumer group: eh-local
> Start from: Stored checkpoint
Reads from: profile orders-dev, namespace orders.servicebus.windows.net, event hub orders
Same as: eh read --profile orders-dev --consumer-group=eh-local
? Start reading? (Y/n)
```

Copy the `Same as:` line to run it again without the menu.

## In eh-gui

- Open the connection sheet and pick a profile in the Profile picker at the top. "Saves to"
  shows the `config.json` that Save config writes, Connect captures into that profile's data
  folder, and the connection pill shows the profile's name.
- To make a profile from the form, fill in the sheet, press **Save as profile…**, type a name,
  and press Save profile (or Return). A connection string already stored for the form is
  copied into the new profile, and the sheet opens it.
- Profiles saved by `eh` show up in eh-gui's picker, and eh-gui's show up in `eh profile list`.

## Keep them tidy

- One profile per event hub and environment: `orders-dev`, `orders-prod`, `billing-dev`. A
  data folder holds one event hub, so make a new profile for another hub instead of pointing an
  old one at it.
- Names use letters, digits, `-`, `_`, and `.`, and ignore case: `Orders` and `orders` are the
  same profile.
- To bring in a folder you already use with `eh`, close `eh` and eh-gui, then move or copy that
  folder into the workspace. The folder's name becomes the profile's name, and its archive comes
  along.
- To rename a profile, close `eh` and eh-gui and rename its folder. To remove one, delete its
  folder; that deletes its archive and exports too.
- To keep profiles somewhere else, set `EH_WORKSPACE` to an absolute folder only you can write
  to (not a repository, a synced folder, or a network share).
- An absolute data folder, export folder, or received message path belongs in a profile. A
  `config.json` in a working folder may name only folders inside that folder, and `eh` refuses
  to run with one that names another. To keep a profile's archive on another disk, set
  `storage.base_data_folder` in the profile's `config.json` to the absolute path.

## After rotating a key

The string sits in the profile's `.eh-data/secrets.redb`, which is not encrypted. After you
rotate the key, delete that file and save the profile again with the new string:

```bash
export EH_CONNECTION_STRING='<the new string>'
eh read --profile orders-dev --save-config
unset EH_CONNECTION_STRING
```

Entra ID profiles store no secret, so there is nothing to rotate in them.
