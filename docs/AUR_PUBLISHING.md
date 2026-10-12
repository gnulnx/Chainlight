# Publishing Chainlight to the AUR

Installing Chainlight does not require an AUR account. Maintaining its AUR
listing does. The tested package recipe and metadata are in `integrations/arch`;
the recipe downloads the versioned source from GitHub releases.

## One-time account setup

1. Register your own account at <https://aur.archlinux.org/register> and complete
   its verification. Choose the username that should maintain Chainlight.
2. Add your publishing SSH **public** key to your AUR account. A dedicated key
   can be created with `ssh-keygen -t ed25519 -f ~/.ssh/chainlight_aur`.
   Only the `.pub` file goes into the account; the private key stays local.
3. Authenticate with `ssh -i ~/.ssh/chainlight_aur aur@aur.archlinux.org help`.
   Verify the server host key using Arch's published fingerprints when prompted.

## Package submission

Publish the matching GitHub release first. Then clone the AUR package repository
using the publishing key, copy the tested recipe and metadata, and push:

```sh
GIT_SSH_COMMAND='ssh -i ~/.ssh/chainlight_aur -o IdentitiesOnly=yes' \
  git -c init.defaultBranch=master clone ssh://aur@aur.archlinux.org/chainlight.git
cd chainlight
# Copy the reviewed release's PKGBUILD and matching .SRCINFO here.
git add PKGBUILD .SRCINFO
git commit -m 'Publish Chainlight 0.3.0'
GIT_SSH_COMMAND='ssh -i ~/.ssh/chainlight_aur -o IdentitiesOnly=yes' git push origin master
```

Use `makepkg --printsrcinfo > .SRCINFO` to regenerate metadata after any recipe
change. Always test the source download and checksum, build, install, update,
launcher and removal before publishing an update. Do not upload source archives,
compiled packages, private keys, node cookies or Bitcoin data into the AUR Git
repository.

The account owner can install via `yay -S chainlight` after the listing is live.
An AUR listing is a community package, not official inclusion in Omarchy.

Reference: [Arch AUR submission guidelines](https://wiki.archlinux.org/title/AUR_submission_guidelines).
