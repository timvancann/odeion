# Run from source.
run:
    cargo run --release

# Build target/Riff.app.
bundle:
    tools/bundle.sh

# Build the app and install it in ~/Applications.
install: bundle
    rm -rf ~/Applications/Riff.app
    mkdir -p ~/Applications
    cp -R target/Riff.app ~/Applications/
