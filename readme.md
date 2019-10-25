# cargo-uf2
Replaces the cargo run command to flash to uf2 bootloaders.

## install
Not published yet, for now
`cargo install --git https://github.com/jacobrosenthal/cargo-uf2`

## use
Head to a uf2 project directory, you can skip running cargo build, we'll do that for you, and you can pass all the usual commands to us like --example and --release, if the builds succeeds we open the usb device and copy the file over.
```bash
$ cargo uf2 --example ferris_img --release --pid 0x003d --vid 0x239a
    Finished release [optimized + debuginfo] target(s) in 0.28s
    Flashing "./target/thumbv7em-none-eabihf/release/examples/ferris_img"
Success
    Finished in 0.037s
```
Optionally you can leave off pid and vid and it'll attempt to query any hid devices with the bininfo packet and write to the first one that responds
```bash
$ cargo uf2 --example ferris_img --release
    Finished release [optimized + debuginfo] target(s) in 0.24s
no vid/pid provided..
trying "" "Apple Internal Keyboard / Trackpad"
trying "Adafruit Industries" "PyGamer"
    Flashing "./target/thumbv7em-none-eabihf/release/examples/ferris_img"
Success
    Finished in 0.034s
```
If it cant find a device, make sure your device is in a bootloader mode. On the PyGamer, 2 button presses enables a blue and green screen that says PyGamer.
```bash
$ cargo uf2 --example ferris_img --release
    Finished release [optimized + debuginfo] target(s) in 0.20s
no vid/pid provided..
trying "" "Apple Internal Keyboard / Trackpad"
trying "" "Keyboard Backlight"
trying "" "Apple Internal Keyboard / Trackpad"
trying "" "Apple Internal Keyboard / Trackpad"
thread 'main' panicked at 'Are you sure device is plugged in and in uf2 mode?', src/libcore/option.rs:1166:5

```

If you find an error, be sure to run with debug to see where in the process it failed `RUST_LOG=debug cargo uf2 --release --example ferris_img`