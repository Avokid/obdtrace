# OBDTrace Linux build dependencies

Target: `x86_64-unknown-linux-gnu`. Generated from `Cargo.lock` with normal and build edges.
Includes build scripts and procedural macros, not just code linked into the executable.
Excludes development-only and other-platform dependencies.

Build: Rust/Cargo, a C compiler and linker, pkg-config, and D-Bus development headers.
The Ubuntu CI build installs `pkg-config libdbus-1-dev libudev-dev`.
Python is used to package and test the archive, not to run OBDTrace.

The first Ubuntu 22.04 x86-64 binary directly needs `libdbus-1.so.3`,
`libgcc_s.so.1`, `libm.so.6` and `libc.so.6` (ELF `DT_NEEDED`).
BlueZ and a working system D-Bus are needed for Bluetooth. SQLite is bundled
via `libsqlite3-sys`; the browser UI embeds i18next 27.2.6 (MIT).

The native archive includes license files under `licenses/`, plus the application
and i18next licenses at its root. Source links refer to the exact crate versions.
For MPL-2.0 `serialport`, the corresponding unmodified source is available
from the versioned download link below.

| Crate | Version | Relationship | Declared license | Source |
| --- | --- | --- | --- | --- |
| anstream | 1.0.0 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/anstream/anstream-1.0.0.crate) |
| anstyle | 1.0.14 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/anstyle/anstyle-1.0.14.crate) |
| anstyle-parse | 1.0.0 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/anstyle-parse/anstyle-parse-1.0.0.crate) |
| anstyle-query | 1.1.5 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/anstyle-query/anstyle-query-1.1.5.crate) |
| anyhow | 1.0.104 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/anyhow/anyhow-1.0.104.crate) |
| async-trait | 0.1.92 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/async-trait/async-trait-0.1.92.crate) |
| atomic-waker | 1.1.2 | Transitive | Apache-2.0 OR MIT | [source](https://static.crates.io/crates/atomic-waker/atomic-waker-1.1.2.crate) |
| axum | 0.8.9 | Direct | MIT | [source](https://static.crates.io/crates/axum/axum-0.8.9.crate) |
| axum-core | 0.5.6 | Transitive | MIT | [source](https://static.crates.io/crates/axum-core/axum-core-0.5.6.crate) |
| base64 | 0.22.1 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/base64/base64-0.22.1.crate) |
| bitflags | 1.3.2 | Transitive | MIT/Apache-2.0 | [source](https://static.crates.io/crates/bitflags/bitflags-1.3.2.crate) |
| bitflags | 2.13.2 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/bitflags/bitflags-2.13.2.crate) |
| block-buffer | 0.10.4 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/block-buffer/block-buffer-0.10.4.crate) |
| bluez-async | 0.8.2 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/bluez-async/bluez-async-0.8.2.crate) |
| bluez-generated | 0.4.0 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/bluez-generated/bluez-generated-0.4.0.crate) |
| btleplug | 0.11.8 | Direct | MIT/Apache-2.0/BSD-3-Clause | [source](https://static.crates.io/crates/btleplug/btleplug-0.11.8.crate) |
| bytes | 1.12.2 | Transitive | MIT | [source](https://static.crates.io/crates/bytes/bytes-1.12.2.crate) |
| cc | 1.11.2 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/cc/cc-1.11.2.crate) |
| cfg-if | 1.0.5 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/cfg-if/cfg-if-1.0.5.crate) |
| cfg_aliases | 0.2.2 | Transitive | MIT | [source](https://static.crates.io/crates/cfg_aliases/cfg_aliases-0.2.2.crate) |
| clap | 4.6.8 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/clap/clap-4.6.8.crate) |
| clap_builder | 4.6.7 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/clap_builder/clap_builder-4.6.7.crate) |
| clap_derive | 4.6.8 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/clap_derive/clap_derive-4.6.8.crate) |
| clap_lex | 1.1.2 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/clap_lex/clap_lex-1.1.2.crate) |
| colorchoice | 1.0.5 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/colorchoice/colorchoice-1.0.5.crate) |
| cpufeatures | 0.2.17 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/cpufeatures/cpufeatures-0.2.17.crate) |
| crossbeam-utils | 0.8.23 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/crossbeam-utils/crossbeam-utils-0.8.23.crate) |
| crypto-common | 0.1.7 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/crypto-common/crypto-common-0.1.7.crate) |
| dashmap | 6.2.1 | Transitive | MIT | [source](https://static.crates.io/crates/dashmap/dashmap-6.2.1.crate) |
| data-encoding | 2.11.1 | Transitive | MIT | [source](https://static.crates.io/crates/data-encoding/data-encoding-2.11.1.crate) |
| dbus | 0.9.12 | Transitive | Apache-2.0/MIT | [source](https://static.crates.io/crates/dbus/dbus-0.9.12.crate) |
| dbus-tokio | 0.7.6 | Transitive | Apache-2.0/MIT | [source](https://static.crates.io/crates/dbus-tokio/dbus-tokio-0.7.6.crate) |
| deranged | 0.5.9 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/deranged/deranged-0.5.9.crate) |
| digest | 0.10.7 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/digest/digest-0.10.7.crate) |
| either | 1.19.0 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/either/either-1.19.0.crate) |
| errno | 0.3.15 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/errno/errno-0.3.15.crate) |
| fallible-iterator | 0.3.0 | Transitive | MIT/Apache-2.0 | [source](https://static.crates.io/crates/fallible-iterator/fallible-iterator-0.3.0.crate) |
| fallible-streaming-iterator | 0.1.9 | Transitive | MIT/Apache-2.0 | [source](https://static.crates.io/crates/fallible-streaming-iterator/fallible-streaming-iterator-0.1.9.crate) |
| fastrand | 2.5.0 | Transitive | Apache-2.0 OR MIT | [source](https://static.crates.io/crates/fastrand/fastrand-2.5.0.crate) |
| find-msvc-tools | 0.1.15 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/find-msvc-tools/find-msvc-tools-0.1.15.crate) |
| foldhash | 0.2.0 | Transitive | Zlib | [source](https://static.crates.io/crates/foldhash/foldhash-0.2.0.crate) |
| form_urlencoded | 1.2.2 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/form_urlencoded/form_urlencoded-1.2.2.crate) |
| fs2 | 0.4.3 | Direct | MIT/Apache-2.0 | [source](https://static.crates.io/crates/fs2/fs2-0.4.3.crate) |
| futures | 0.3.34 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/futures/futures-0.3.34.crate) |
| futures-channel | 0.3.34 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/futures-channel/futures-channel-0.3.34.crate) |
| futures-core | 0.3.34 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/futures-core/futures-core-0.3.34.crate) |
| futures-executor | 0.3.34 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/futures-executor/futures-executor-0.3.34.crate) |
| futures-io | 0.3.34 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/futures-io/futures-io-0.3.34.crate) |
| futures-macro | 0.3.34 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/futures-macro/futures-macro-0.3.34.crate) |
| futures-sink | 0.3.34 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/futures-sink/futures-sink-0.3.34.crate) |
| futures-task | 0.3.34 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/futures-task/futures-task-0.3.34.crate) |
| futures-util | 0.3.34 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/futures-util/futures-util-0.3.34.crate) |
| generic-array | 0.14.7 | Transitive | MIT | [source](https://static.crates.io/crates/generic-array/generic-array-0.14.7.crate) |
| getrandom | 0.2.17 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/getrandom/getrandom-0.2.17.crate) |
| getrandom | 0.3.4 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/getrandom/getrandom-0.3.4.crate) |
| getrandom | 0.4.3 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/getrandom/getrandom-0.4.3.crate) |
| hashbrown | 0.14.5 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/hashbrown/hashbrown-0.14.5.crate) |
| hashbrown | 0.17.2 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/hashbrown/hashbrown-0.17.2.crate) |
| hashlink | 0.12.2 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/hashlink/hashlink-0.12.2.crate) |
| heck | 0.5.0 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/heck/heck-0.5.0.crate) |
| hmac | 0.12.1 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/hmac/hmac-0.12.1.crate) |
| http | 1.5.0 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/http/http-1.5.0.crate) |
| http-body | 1.1.0 | Transitive | MIT | [source](https://static.crates.io/crates/http-body/http-body-1.1.0.crate) |
| http-body-util | 0.1.5 | Transitive | MIT | [source](https://static.crates.io/crates/http-body-util/http-body-util-0.1.5.crate) |
| httparse | 1.10.1 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/httparse/httparse-1.10.1.crate) |
| httpdate | 1.0.3 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/httpdate/httpdate-1.0.3.crate) |
| hyper | 1.12.0 | Transitive | MIT | [source](https://static.crates.io/crates/hyper/hyper-1.12.0.crate) |
| hyper-util | 0.1.21 | Transitive | MIT | [source](https://static.crates.io/crates/hyper-util/hyper-util-0.1.21.crate) |
| is_terminal_polyfill | 1.70.2 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/is_terminal_polyfill/is_terminal_polyfill-1.70.2.crate) |
| itertools | 0.14.0 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/itertools/itertools-0.14.0.crate) |
| itoa | 1.0.19 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/itoa/itoa-1.0.19.crate) |
| lazy_static | 1.5.1 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/lazy_static/lazy_static-1.5.1.crate) |
| libc | 0.2.191 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/libc/libc-0.2.191.crate) |
| libdbus-sys | 0.2.7 | Transitive | Apache-2.0/MIT | [source](https://static.crates.io/crates/libdbus-sys/libdbus-sys-0.2.7.crate) |
| libsqlite3-sys | 0.39.1 | Transitive | MIT | [source](https://static.crates.io/crates/libsqlite3-sys/libsqlite3-sys-0.39.1.crate) |
| linux-raw-sys | 0.12.1 | Transitive | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [source](https://static.crates.io/crates/linux-raw-sys/linux-raw-sys-0.12.1.crate) |
| lock_api | 0.4.14 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/lock_api/lock_api-0.4.14.crate) |
| log | 0.4.35 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/log/log-0.4.35.crate) |
| matchers | 0.2.0 | Transitive | MIT | [source](https://static.crates.io/crates/matchers/matchers-0.2.0.crate) |
| matchit | 0.8.4 | Transitive | MIT AND BSD-3-Clause | [source](https://static.crates.io/crates/matchit/matchit-0.8.4.crate) |
| memchr | 2.8.3 | Transitive | Unlicense OR MIT | [source](https://static.crates.io/crates/memchr/memchr-2.8.3.crate) |
| mime | 0.3.18 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/mime/mime-0.3.18.crate) |
| mio | 1.2.5 | Transitive | MIT | [source](https://static.crates.io/crates/mio/mio-1.2.5.crate) |
| mio-serial | 5.0.7 | Transitive | MIT | [source](https://static.crates.io/crates/mio-serial/mio-serial-5.0.7.crate) |
| nix | 0.26.4 | Transitive | MIT | [source](https://static.crates.io/crates/nix/nix-0.26.4.crate) |
| nix | 0.31.4 | Transitive | MIT | [source](https://static.crates.io/crates/nix/nix-0.31.4.crate) |
| nu-ansi-term | 0.50.3 | Transitive | MIT | [source](https://static.crates.io/crates/nu-ansi-term/nu-ansi-term-0.50.3.crate) |
| num-conv | 0.2.2 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/num-conv/num-conv-0.2.2.crate) |
| once_cell | 1.21.4 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/once_cell/once_cell-1.21.4.crate) |
| parking_lot | 0.12.5 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/parking_lot/parking_lot-0.12.5.crate) |
| parking_lot_core | 0.9.12 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/parking_lot_core/parking_lot_core-0.9.12.crate) |
| pem | 3.0.6 | Transitive | MIT | [source](https://static.crates.io/crates/pem/pem-3.0.6.crate) |
| percent-encoding | 2.3.2 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/percent-encoding/percent-encoding-2.3.2.crate) |
| pin-project-lite | 0.2.17 | Transitive | Apache-2.0 OR MIT | [source](https://static.crates.io/crates/pin-project-lite/pin-project-lite-0.2.17.crate) |
| pkg-config | 0.3.34 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/pkg-config/pkg-config-0.3.34.crate) |
| powerfmt | 0.2.1 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/powerfmt/powerfmt-0.2.1.crate) |
| ppv-lite86 | 0.2.21 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/ppv-lite86/ppv-lite86-0.2.21.crate) |
| proc-macro2 | 1.0.107 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/proc-macro2/proc-macro2-1.0.107.crate) |
| quote | 1.0.47 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/quote/quote-1.0.47.crate) |
| rand | 0.9.5 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/rand/rand-0.9.5.crate) |
| rand_chacha | 0.9.0 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/rand_chacha/rand_chacha-0.9.0.crate) |
| rand_core | 0.9.5 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/rand_core/rand_core-0.9.5.crate) |
| rcgen | 0.13.2 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/rcgen/rcgen-0.13.2.crate) |
| regex-automata | 0.4.19 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/regex-automata/regex-automata-0.4.19.crate) |
| regex-syntax | 0.8.12 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/regex-syntax/regex-syntax-0.8.12.crate) |
| ring | 0.17.14 | Transitive | Apache-2.0 AND ISC | [source](https://static.crates.io/crates/ring/ring-0.17.14.crate) |
| rusqlite | 0.41.1 | Direct | MIT | [source](https://static.crates.io/crates/rusqlite/rusqlite-0.41.1.crate) |
| rustix | 1.1.6 | Transitive | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [source](https://static.crates.io/crates/rustix/rustix-1.1.6.crate) |
| rustls | 0.23.46 | Transitive | Apache-2.0 OR ISC OR MIT | [source](https://static.crates.io/crates/rustls/rustls-0.23.46.crate) |
| rustls-pki-types | 1.16.0 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/rustls-pki-types/rustls-pki-types-1.16.0.crate) |
| rustls-webpki | 0.103.17 | Transitive | ISC | [source](https://static.crates.io/crates/rustls-webpki/rustls-webpki-0.103.17.crate) |
| ryu | 1.0.23 | Transitive | Apache-2.0 OR BSL-1.0 | [source](https://static.crates.io/crates/ryu/ryu-1.0.23.crate) |
| scopeguard | 1.2.0 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/scopeguard/scopeguard-1.2.0.crate) |
| serde | 1.0.229 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/serde/serde-1.0.229.crate) |
| serde-xml-rs | 0.8.2 | Transitive | MIT | [source](https://static.crates.io/crates/serde-xml-rs/serde-xml-rs-0.8.2.crate) |
| serde_core | 1.0.229 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/serde_core/serde_core-1.0.229.crate) |
| serde_derive | 1.0.229 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/serde_derive/serde_derive-1.0.229.crate) |
| serde_json | 1.0.154 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/serde_json/serde_json-1.0.154.crate) |
| serde_path_to_error | 0.1.20 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/serde_path_to_error/serde_path_to_error-0.1.20.crate) |
| serde_urlencoded | 0.7.1 | Transitive | MIT/Apache-2.0 | [source](https://static.crates.io/crates/serde_urlencoded/serde_urlencoded-0.7.1.crate) |
| serialport | 4.10.1 | Transitive | MPL-2.0 | [source](https://static.crates.io/crates/serialport/serialport-4.10.1.crate) |
| sha1 | 0.10.7 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/sha1/sha1-0.10.7.crate) |
| sha2 | 0.10.9 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/sha2/sha2-0.10.9.crate) |
| sharded-slab | 0.1.7 | Transitive | MIT | [source](https://static.crates.io/crates/sharded-slab/sharded-slab-0.1.7.crate) |
| shlex | 2.0.1 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/shlex/shlex-2.0.1.crate) |
| signal-hook-registry | 1.4.8 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/signal-hook-registry/signal-hook-registry-1.4.8.crate) |
| slab | 0.4.12 | Transitive | MIT | [source](https://static.crates.io/crates/slab/slab-0.4.12.crate) |
| smallvec | 1.16.6 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/smallvec/smallvec-1.16.6.crate) |
| socket2 | 0.6.5 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/socket2/socket2-0.6.5.crate) |
| static_assertions | 1.1.0 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/static_assertions/static_assertions-1.1.0.crate) |
| strsim | 0.11.1 | Transitive | MIT | [source](https://static.crates.io/crates/strsim/strsim-0.11.1.crate) |
| subtle | 2.6.1 | Transitive | BSD-3-Clause | [source](https://static.crates.io/crates/subtle/subtle-2.6.1.crate) |
| syn | 2.0.119 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/syn/syn-2.0.119.crate) |
| syn | 3.0.10 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/syn/syn-3.0.10.crate) |
| sync_wrapper | 1.0.2 | Transitive | Apache-2.0 | [source](https://static.crates.io/crates/sync_wrapper/sync_wrapper-1.0.2.crate) |
| tempfile | 3.27.0 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/tempfile/tempfile-3.27.0.crate) |
| thiserror | 2.0.21 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/thiserror/thiserror-2.0.21.crate) |
| thiserror-impl | 2.0.21 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/thiserror-impl/thiserror-impl-2.0.21.crate) |
| thread_local | 1.1.10 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/thread_local/thread_local-1.1.10.crate) |
| time | 0.3.55 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/time/time-0.3.55.crate) |
| time-core | 0.1.9 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/time-core/time-core-0.1.9.crate) |
| time-macros | 0.2.32 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/time-macros/time-macros-0.2.32.crate) |
| tokio | 1.54.1 | Direct | MIT | [source](https://static.crates.io/crates/tokio/tokio-1.54.1.crate) |
| tokio-macros | 2.7.2 | Transitive | MIT | [source](https://static.crates.io/crates/tokio-macros/tokio-macros-2.7.2.crate) |
| tokio-rustls | 0.26.6 | Direct | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/tokio-rustls/tokio-rustls-0.26.6.crate) |
| tokio-serial | 5.5.0 | Direct | MIT | [source](https://static.crates.io/crates/tokio-serial/tokio-serial-5.5.0.crate) |
| tokio-stream | 0.1.19 | Transitive | MIT | [source](https://static.crates.io/crates/tokio-stream/tokio-stream-0.1.19.crate) |
| tokio-tungstenite | 0.29.0 | Transitive | MIT | [source](https://static.crates.io/crates/tokio-tungstenite/tokio-tungstenite-0.29.0.crate) |
| tokio-util | 0.7.20 | Transitive | MIT | [source](https://static.crates.io/crates/tokio-util/tokio-util-0.7.20.crate) |
| tower | 0.5.3 | Transitive | MIT | [source](https://static.crates.io/crates/tower/tower-0.5.3.crate) |
| tower-layer | 0.3.3 | Transitive | MIT | [source](https://static.crates.io/crates/tower-layer/tower-layer-0.3.3.crate) |
| tower-service | 0.3.3 | Transitive | MIT | [source](https://static.crates.io/crates/tower-service/tower-service-0.3.3.crate) |
| tracing | 0.1.44 | Direct | MIT | [source](https://static.crates.io/crates/tracing/tracing-0.1.44.crate) |
| tracing-attributes | 0.1.31 | Transitive | MIT | [source](https://static.crates.io/crates/tracing-attributes/tracing-attributes-0.1.31.crate) |
| tracing-core | 0.1.36 | Transitive | MIT | [source](https://static.crates.io/crates/tracing-core/tracing-core-0.1.36.crate) |
| tracing-log | 0.2.0 | Transitive | MIT | [source](https://static.crates.io/crates/tracing-log/tracing-log-0.2.0.crate) |
| tracing-subscriber | 0.3.23 | Direct | MIT | [source](https://static.crates.io/crates/tracing-subscriber/tracing-subscriber-0.3.23.crate) |
| tungstenite | 0.29.0 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/tungstenite/tungstenite-0.29.0.crate) |
| typenum | 1.20.1 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/typenum/typenum-1.20.1.crate) |
| unescaper | 0.1.10 | Transitive | MIT OR GPL-3.0-only | [source](https://static.crates.io/crates/unescaper/unescaper-0.1.10.crate) |
| unicode-ident | 1.0.27 | Transitive | (MIT OR Apache-2.0) AND Unicode-3.0 | [source](https://static.crates.io/crates/unicode-ident/unicode-ident-1.0.27.crate) |
| untrusted | 0.9.0 | Transitive | ISC | [source](https://static.crates.io/crates/untrusted/untrusted-0.9.0.crate) |
| utf8parse | 0.2.2 | Transitive | Apache-2.0 OR MIT | [source](https://static.crates.io/crates/utf8parse/utf8parse-0.2.2.crate) |
| uuid | 1.29.3 | Direct | Apache-2.0 OR MIT | [source](https://static.crates.io/crates/uuid/uuid-1.29.3.crate) |
| vcpkg | 0.2.15 | Transitive | MIT/Apache-2.0 | [source](https://static.crates.io/crates/vcpkg/vcpkg-0.2.15.crate) |
| version_check | 0.9.5 | Transitive | MIT/Apache-2.0 | [source](https://static.crates.io/crates/version_check/version_check-0.9.5.crate) |
| xml | 1.4.0 | Transitive | MIT | [source](https://static.crates.io/crates/xml/xml-1.4.0.crate) |
| yasna | 0.5.2 | Transitive | MIT OR Apache-2.0 | [source](https://static.crates.io/crates/yasna/yasna-0.5.2.crate) |
| zerocopy | 0.8.63 | Transitive | BSD-2-Clause OR Apache-2.0 OR MIT | [source](https://static.crates.io/crates/zerocopy/zerocopy-0.8.63.crate) |
| zeroize | 1.9.2 | Transitive | Apache-2.0 OR MIT | [source](https://static.crates.io/crates/zeroize/zeroize-1.9.2.crate) |
| zmij | 1.0.23 | Transitive | MIT | [source](https://static.crates.io/crates/zmij/zmij-1.0.23.crate) |
