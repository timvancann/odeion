# Run from source.
run:
    cargo run --release

# Build target/Odeion.app.
bundle:
    tools/bundle.sh

# Build the app and install it in ~/Applications.
install: bundle
    rm -rf ~/Applications/Odeion.app
    mkdir -p ~/Applications
    cp -R target/Odeion.app ~/Applications/
