# Birch GitHub repository setup and initial publication

The verified destination for this source is the existing **private** repository `oneshubhamt/birch` on GitHub, with `main` as the default branch. The Git-ready distribution contains a local Git history with one initial commit and an `origin` remote already pointing to this repository. No publishing token or user data is included in the ZIP.

## Publish the complete local repository

On your Mac, install the Xcode Command Line Tools if Git is unavailable (`xcode-select --install`). GitHub CLI (`gh`) simplifies authentication; install it with `brew install gh` if you already use Homebrew, or follow the GitHub CLI installer documentation.

1. Download the prepared repository ZIP into `~/Downloads`.
2. Open Terminal and run (substitute the exact downloaded archive filename):

```sh
cd ~/Downloads
unzip -q birch_rustnote_v0.12.6_2b.43.3_git_ready.zip
cd birch
git status
git remote -v
```

3. Authorize GitHub CLI to access your account and configure Git to use that login (follow the browser prompts):

```sh
gh auth login
gh auth setup-git
```

4. Publish the complete prepared commit to the empty private repository:

```sh
git push -u origin main
```

5. Verify the files and green checks in [the Birch GitHub repository](https://github.com/oneshubhamt/birch). The first CI run may detect existing source issues or need fixes; don't assume it passed merely because the push succeeded.

The command is a **normal initial push**, not a force push. If `main` is no longer empty, do not force-update it; inspect the remote changes before publishing.

## Recommended GitHub UI configuration (after first CI run)

In repository **Settings**, preserve `Private` visibility. Enable dependency graph, Dependabot alerts/security updates and private vulnerability reporting if supported for the account. Under **Rules → Rulesets**, protect `main`: prevent force pushes and deletion, require pull requests and relevant passing CI checks. Select check names only after the first successful workflow run (GitHub can then discover them). The installed GitHub integration may not have permission to change these administrative settings; verify manually.

Do not add GitHub Actions secrets for the current workflow. The CI needs read-only contents access only. Do not automatically create signed app releases, publish binaries, or turn on automatic Dependabot merges.

## Future routine work

```sh
cd ~/Downloads/birch
git pull --ff-only
# Edit code and run the relevant QA checks.
git add .
git commit -m "Describe the audited change"
git push
```

Once branch protection is enabled, use feature branches and pull requests rather than pushing new changes directly to `main`.
