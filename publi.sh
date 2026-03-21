#!/usr/bin/bash

set -euf pipefail

VERSION=$(cargo metadata --format-version=1 --no-deps | jq '.packages[0].version' --raw-output)
TAG="v${VERSION}"
podman login --authfile ~/.config/containers/auth.json
podman build --no-cache -t registry.sumebrius.com/library/ai-operator:latest -t registry.sumebrius.com/library/ai-operator:${VERSION} .
podman push registry.sumebrius.com/library/ai-operator:latest registry.sumebrius.com/library/ai-operator:${VERSION}

set +e
git show-ref --tags ${TAG} --quiet
TAGGED=$?
set -e
if [ $TAGGED -eq 0 ]
then
    echo "Tag ${TAG} already exists"
    exit 0
fi

git tag ${TAG}
git push origin ${TAG}