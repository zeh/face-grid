# Face Grid

A simple command line application in Rust to create a grid of face images. Made as an alternative fork of [face-stack](https://github.com/zeh/face-stack).

Unless you're debugging something, I recommend running with `--release` so everything is faster.

* Run: `cargo run --release`
* Run with parameters: `cargo run --release -- --input /something/*.jpg --cell-size 1024x1024 --columns 10 --max-images 10 --output file.png`
* See basic parameters: `cargo run --release -- --help`
