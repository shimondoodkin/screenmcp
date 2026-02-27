#!/bin/bash
set -e

cd "$(dirname "$0")"

echo "Building @screenmcp/sdk..."
npm run build

#echo "logging in..."
#npm login

echo "Publishing to npm..."
npm publish --access public

echo "Done! Published @screenmcp/sdk v$(node -p "require('./package.json').version")"
