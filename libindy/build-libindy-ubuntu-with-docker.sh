#!/bin/bash

cd ..

# Generate temporary .dockerignore to limit context size sent to Docker daemon
echo .git/ >> .dockerignore
echo Specs >> .dockerignore
echo ci >> .dockerignore
echo cli >> .dockerignore
echo doc >> .dockerignore
echo experimental >> .dockerignore
echo libnullpay >> .dockerignore
echo samples >> .dockerignore
echo vcx >> .dockerignore

# Build Docker image
docker build -t build-libindy-ubuntu -f libindy/build-libindy-ubuntu.dockerfile .

# Remove temporary .dockerignore
rm .dockerignore

# Return to original directory
cd libindy
