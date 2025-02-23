# Update version numbers of various core-crypto components.
# Prerequisites: installed `sed` (`gnu-sed` aliased to `sed` on mac os).

# Usage (to update to 0.0.1): sh update-versions.sh 0.0.1
new_version=$1
for crate in litt \
             index \
             shared \
             search; do
    sed -i "0,/^version = \"[^\"]\+\"/{s//version = \"${new_version}\"/;b;}" $crate/Cargo.toml
done
