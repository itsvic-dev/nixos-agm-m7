# mm-plugin-mtk-ccci

A ModemManager plugin for the integrated MT6739 modem of the AGM M7 (firmware MOLY.LR12A.R2.MP.V96.6).

The plugin uses the `mt6739-ccci` kernel driver ports:

- `/dev/wwan0at0`: the AT port (WWAN port type AT). It appears after the modem boot handshake.
- `wwan0`: the raw-IP net device (ccmni0). It appears at probe time.

`package.nix` builds ModemManager 1.24 from nixpkgs with this plugin added. The flake exposes it as `packages.aarch64-linux.modemmanager-agm`.

## What the plugin does

- It matches ports of the `wwan` and `net` subsystems that have the `mt6739-ccci` driver.
- It reports the capabilities GSM/UMTS and LTE.
- It reads and sets the access technologies with `+ERAT`. Preference 4 is "LTE preferred" (the stock RIL sets `+ERAT=6,4`).
- It ignores the MediaTek URCs that ModemManager does not use (`+EIND`, `+ESIMIND`, `+EUSIM`, `+ECPIN`, `+ETESTSIM`, `+EIRAT`, `+CEN1`, `+CEN2`, `+CNEMS1`, `+CNEMIU`, `+CIREPI`, `+EDRETRY`, `+EWIFIPDNACT`, `+EAPNINFO`, `+EIMSPDIS`, `+ECSQ`, `+EIF`, `+EPDN`).
- It connects and disconnects data with the MediaTek PDN commands. The data goes through `wwan0` with a static IPv4 configuration.

## AT sequence

When the modem is enabled (after `+CFUN=1`), the plugin sends:

```
AT+EIFCONFIG=3
AT+EIAREG=1
AT+EDALLOW=1
```

Errors are logged, but enabling continues.

To connect with APN `<apn>`, user `<u>` and password `<p>`:

```
AT+EAPNSET                           (clears the APN table)
AT+EAPNSET="<apn>",1,"<u>","<p>"
AT+EAPNSET="<apn>",2,"type=default,supl,hipri;protocol=IP;roaming_protocol=IP;authtype=<a>;carrier_enabled=1;max_conns=0;max_conns_time=0;wait_time=0;bearer_bitmask=2147352575;inactive_timer=0"
AT+EIAAPN="<apn>",0                  (errors are ignored)
AT+CGATT?                            (then AT+CGATT=1 if the modem is not attached)
AT+EAPNACT=1,"<apn>","default",0     (timeout 60 s)
AT+EIF=<ifid>,"ifst","up",1
AT+EPDN=<aid>,"ifst",20
AT+CGCONTRDP
```

- `<a>` is the Android authtype: 0 none, 1 PAP, 2 CHAP, 3 PAP or CHAP. If the auth type is not set, the plugin uses 2 when a user or password is set, else 0.
- `<aid>` comes from `+CGEV: ME PDN ACT <aid>` or `+EPDN: <aid>, "new", ...`. If neither comes, the plugin finds the `+CGCONTRDP` entry whose APN starts with `<apn>`.
- `<ifid>` and the IPv4 address come from `+EIF: <ifid>, ifup, 1, <ipv4>, 0` or `+EPDN: <aid>, "new", 1, <ifid>, <mtu>, <iptype>, "<ipv4>"`. The plugin waits up to 10 s for them after `+EAPNACT`.
- The ccmni index is `<ifid> % 100`. The modem picks it, and it changes when old PDNs are still open. The kernel learns it from RPC 0x400d and maps it to `wwan0`.
- The modem answers `+EPDN` only after the `+EIF` confirmation. The stock netagent sends `+EIF` on a second AT channel; with one channel, `+EIF` must come first.
- The `+EAPNSET` steps and `+EPDN` are not fatal. Adding an APN that is already in the table fails with `CME ERROR: 5841`.
- `+CGCONTRDP` gives DNS and MTU.
- If a step fails after `+EAPNACT=1`, the plugin sends `AT+EAPNACT=0,<aid>`.

The bearer reports method `static`, the address with prefix 32, no gateway, DNS and MTU. The profile ID of the bearer is `<aid>`, so `+CGEV: NW PDN DEACT <aid>` and `+CGEV: ME PDN DEACT <aid>` mark the bearer as disconnected. The plugin does not poll the connection status.

To disconnect:

```
AT+EAPNACT=0,<aid>
AT+EIF=<ifid>,"ifst","down",1        (errors are ignored)
```

## udev rules

`77-mm-mtk-ccci.rules`:

- Tags the AT port with `ID_MM_PORT_TYPE_AT_PRIMARY`.
- Sets `ID_MM_PHYSDEV_UID=mt6739-modem` on the AT port and on `wwan0`.

ModemManager also puts both ports in one device without the UID, because both have the platform device `10014000.modem` as the physical device. The default `80-mm-candidate.rules` already sets `ID_MM_CANDIDATE` for `wwan` and `net` devices, and the strict filter policy allows both subsystems. `ID_MM_DEVICE_PROCESS` is not necessary.

## Enable it in NixOS

```nix
{ pkgs, ... }:
{
  networking.modemmanager.enable = true;
  networking.modemmanager.package = pkgs.callPackage ../tools/mm-plugin-mtk-ccci/package.nix { };
}
```

The NixOS module also adds the package to `services.udev.packages`, so the udev rules are installed. NetworkManager enables ModemManager by default.

## Open points

- Not tested on the phone yet.
- The order of the URCs during `+EAPNACT` and `+EPDN` can change. The plugin collects them with URC handlers while it connects. If `+EIF: ifup` does not include the address, the plugin needs `+EPDN: ... "new"` or the address field of `+CGCONTRDP`.
- `+CGEV: ME PDN ACT` comes only if `+CGEREP` is on. ModemManager turns it on during enabling.
- If a PDN for the APN is already active (for example, after a ModemManager restart), `+EAPNACT=1` can fail. The plugin does not deactivate old PDNs first.
- If the network deactivates the PDN, the plugin does not send `+EIF=<ifid>,"ifst","down",1`.
- The meaning of `+EIF: <ifid>, ifup, <n>, ...` field `<n>` (1 = IPv4) and of `+EPDN` field 3 is from observation only.
- IPv6 is not supported. A request for IPv6 only fails. IPv4v6 and "any" use IPv4.
- `+ERAT` preference 3 is read as "4G preferred" too (older MediaTek modems use 3).
- NetworkManager must add the default route on `wwan0` without a gateway. This is not tested.
- Only the shared-module build of ModemManager is supported (`-Dbuiltin_plugins=false`, the nixpkgs default).
