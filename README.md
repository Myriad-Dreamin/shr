# shr

`shr` checks and reports disk space.

## Installation

Install GUI using `cargo`:

```bash
cargo install --git https://github.com/Myriad-Dreamin/shr --locked shr-browser
```

Install CLI using `cargo`:

```bash
cargo install --git https://github.com/Myriad-Dreamin/shr --locked shr-cli
```

## Usage

The `shr` (`shr-cli`) and `shr-browser` commands has same CLI flags.

```bash
shr path
shr-browser path
```

## Todo List

- [ ] Right click to open file/folder.
- [ ] Better GUI, Currently it is a bit ugly.
- [ ] Mobile Support: [See: Slint Mobile Platform.](https://docs.slint.dev/latest/docs/slint/guide/platforms/mobile/)
- [ ] Reduce Memory Usage, Currently GUI costs 1.5GB when scanning a disk containing 4.5m files (seems like 300B per file).

## Development

VSCode:

```bash
code .vscode/shr.code-workspace
```
