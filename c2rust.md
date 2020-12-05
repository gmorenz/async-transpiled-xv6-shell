```
cargo +nightly-2019-12-05 install --git https://github.com/kaspar030/c2rust.git --branch clang-11-fixes c2rust
cargo init .
clang -MJ sh.json sh.c -o sh
# Insert [ at start of sh.json, replace , with ] at end of sh.json
c2rust transpile sh.json
mv sh.rs src/main.rs
cargo add libc
```
