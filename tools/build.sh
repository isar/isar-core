cargo install cross
cross build --release
cross build --release --target x86_64-pc-windows-gnu
cross build --release --target aarch64-unknown-linux-gnu
cross build --release --target aarch64-linux-android-gnu
