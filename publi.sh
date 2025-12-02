#!/usr/bin/bash

set -e

VERSION=$(cargo metadata --format-version=1 --no-deps | jq '.packages[0].version' --raw-output)
podman build -t registry.sumebrius.com/library/ai-operator:latest registry.sumebrius.com/library/ai-operator:${VERSION} .
podman push registry.sumebrius.com/library/ai-operator:latest registry.sumebrius.com/library/ai-operator:${VERSION}
git tag v${VERSION}
git push origin v${VERSION}