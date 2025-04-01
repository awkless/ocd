<!--
SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
SPDX-License-Identifier: MIT or Apache-2.0
-->
# Changelog

All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

## [0.5.0](https://github.com/awkless/ocd/compare/v0.4.0..0.5.0) - 2025-04-01

### Features

- *(bin)* Add deploy command to CLI - ([d32dda7](https://github.com/awkless/ocd/commit/d32dda7ea7ed3f963bfd2cca13be7909bf8f9745))
- *(bin)* Add glob matching to git shortcut command - ([a3fdd9c](https://github.com/awkless/ocd/commit/a3fdd9cd9d1f09dffd0baea0a19c83393aa8122c))
- *(fs)* Add `glob_match` - ([8b89990](https://github.com/awkless/ocd/commit/8b89990a63758029c427324a24ead9b818f08a47))
- *(fs->utils)* Rename `fs` to `utils` - ([d9a05f5](https://github.com/awkless/ocd/commit/d9a05f5fb51bfe71e8287d88a3326ae3b0c860ff))
- *(vcs)* Add repository deployment functionality - ([8397d1b](https://github.com/awkless/ocd/commit/8397d1b9b13890676d14a60ff26ddb9187b31ebd))

### Miscellaneous Chores

- *(cargo)* Add `glob` crate - ([bc45b4a](https://github.com/awkless/ocd/commit/bc45b4a679293981abe8a61db2bd7ca656b915cc))
- *(cargo)* Bump version to 0.5.0 - ([5f17001](https://github.com/awkless/ocd/commit/5f17001b79bd435ade1b0d05a2bdaedac687f5ea))

### Ref

- *(utils)* Improve interface of `glob_match` - ([3774093](https://github.com/awkless/ocd/commit/37740933acf90392d534619c4c061e309b9afe76))
## [0.4.0](https://github.com/awkless/ocd/compare/v0.3.0..v0.4.0) - 2025-03-30

### Documentation

- *(changelog)* Document version 0.4.0 - ([8d39a2b](https://github.com/awkless/ocd/commit/8d39a2b3ca984752aebd7cfd7f2699fb435abd85))

### Features

- *(bin)* Add git shortcut command - ([a79eb9f](https://github.com/awkless/ocd/commit/a79eb9f3e356eea9a45f702c5353d335fb049804))
- *(vcs)* Allow `NodeRepo` and `RootRepo` to make calls to git - ([28a5daa](https://github.com/awkless/ocd/commit/28a5daaa085969447fd9863aa8a7a779af38c413))

### Miscellaneous Chores

- *(cargo)* Bump version and optimize release size - ([2e53e0a](https://github.com/awkless/ocd/commit/2e53e0a5205708ee35be843242a0d6cc1dafa8eb))

### Refactoring

- *(cluster)* Thanks clippy - ([343e901](https://github.com/awkless/ocd/commit/343e90132e554533bdd8559ed662c1e8867ab785))

### Style

- Thanks rustfmt - ([86279b1](https://github.com/awkless/ocd/commit/86279b153e9682703dff147a00707f2ccb45fef1))
## [0.3.0](https://github.com/awkless/ocd/compare/v0.2.0..v0.3.0) - 2025-03-29

### Documentation

- *(bin)* Add a few doc-strings - ([882ecea](https://github.com/awkless/ocd/commit/882ecea025da131fc17e76b25b1a6152e3b39675))
- *(changelog)* Document version 0.3.0 - ([3415d37](https://github.com/awkless/ocd/commit/3415d3748a4f1f3c24d828b93e3d2d356740fdf0))
- *(cluster)* Add doc-comments to public API - ([58a6af1](https://github.com/awkless/ocd/commit/58a6af120fe088b416678d90a583d44da9c48cdd))
- *(fs)* Add documentation to public APIs - ([d833e28](https://github.com/awkless/ocd/commit/d833e28aae84059784caf9b04ea28a3c448824d6))
- *(vcs)* Add doc-comment strings to API - ([191ff35](https://github.com/awkless/ocd/commit/191ff35d211380455fa4cbe430cc5d9ec16a6fa7))

### Features

- *(bin)* Add clone command - ([c10b309](https://github.com/awkless/ocd/commit/c10b3091f6b624f4c460c93033d2ccb176f1792c))
- *(cluster)* Make `Node::url` mandatory - ([e259491](https://github.com/awkless/ocd/commit/e259491d0e35c2f397cc906225475babcfcb1074))
- *(fs)* Add `DirLayout` type - ([c54d4eb](https://github.com/awkless/ocd/commit/c54d4eb853c8f61a0c8b06095ea7e69ea9f839d3))
- *(fs)* Add `read_config` - ([42994a7](https://github.com/awkless/ocd/commit/42994a7fbad7c07bc8b5ac74a6db7267e942f024))
- *(vcs)* Add `Git` type with `Git::bincall` - ([d177ef8](https://github.com/awkless/ocd/commit/d177ef8977dfef4fd6a0d18e5481e97fff4b9441))
- *(vcs)* Add `Git::clone_with_progress` - ([4862035](https://github.com/awkless/ocd/commit/486203546a6ee075137a00a36861b2aef22a961c))
- *(vcs)* Add functionality to implement clone command - ([45b15c5](https://github.com/awkless/ocd/commit/45b15c54a9212da5777be339aee406ad8eb1081a))

### Miscellaneous Chores

- *(cargo)* Bump to version 0.3.0 - ([a72b370](https://github.com/awkless/ocd/commit/a72b3705d174e33541a7e5c1ec0dc730a815a5b6))

### Refactoring

- *(cluster)* Make `Cluster::root` `Cluster::nodes` public - ([7fb7156](https://github.com/awkless/ocd/commit/7fb7156f63439ae9b4a60d99754eb8b2cecbdf3b))
- Thanks clippy - ([54e0619](https://github.com/awkless/ocd/commit/54e0619e12a7a4aeb15be0e853fd53325171bcde))

### Style

- *(fs)* Thanks rustfmt - ([0305823](https://github.com/awkless/ocd/commit/0305823933ac9f5e5b7c44d03f74da867602bcc7))
- *(vcs)* Thanks rustfmt - ([718f7d1](https://github.com/awkless/ocd/commit/718f7d1c2cfa01f1d0f4a11eed99102e279f19ef))
## [0.2.0](https://github.com/awkless/ocd/compare/v0.1.0..v0.2.0) - 2025-03-23

### Documentation

- *(changelog)* Document version 0.2.0 - ([d3ee39a](https://github.com/awkless/ocd/commit/d3ee39ae430992a45a87ade623d1c2d7ea79a764))
- *(contrib)* Use `refactor` instead of `ref` - ([790a3e6](https://github.com/awkless/ocd/commit/790a3e6904cd1b9cd8fc0812ae4b49616b72a5fe))

### Features

- *(bin)* Allow dead code - ([1e033df](https://github.com/awkless/ocd/commit/1e033df6971d31daf1a79694b6ac3c74ba9291a3))
- *(cluster)* Add `Cluster` type - ([594673c](https://github.com/awkless/ocd/commit/594673ccc9f0aa6ea5104eb6ca8a36993360e174))
- *(cluster)* Add `Cluster::from_str` - ([d8f3c75](https://github.com/awkless/ocd/commit/d8f3c75c7ffa6ac1c3dd6dbf36e08a1379febb95))
- *(cluster)* Add `Cluster::to_string` - ([ed22755](https://github.com/awkless/ocd/commit/ed2275511a32ba08bb2a5bfbfdfb2ea8cb6531b1))
- *(cluster)* Add `Root` type - ([a17f88f](https://github.com/awkless/ocd/commit/a17f88f931e0fec0551217b746caa55ec1f9ddec))
- *(cluster)* Add `Cluster::get_root` - ([8eae938](https://github.com/awkless/ocd/commit/8eae93897786c58870c78f18a364d24029935994))
- *(cluster)* Add `Node` type to deserialize into hashmap - ([adf22d5](https://github.com/awkless/ocd/commit/adf22d508dbd0613522914d858f0ad1a0818b3b1))
- *(cluster)* Add acyclic graph check for nodes - ([fb830b9](https://github.com/awkless/ocd/commit/fb830b9bd7bd587c4d46f0b853a6cee3d0f1c90c))
- *(cluster)* Add `Cluster::dependency_iter` - ([3819327](https://github.com/awkless/ocd/commit/3819327fec2b52d6c9be386e337d945a57a749c1))
- *(cluster)* Add `Cluster::get_node` - ([e0c94e5](https://github.com/awkless/ocd/commit/e0c94e5ff800f0e7e547d085874b60ac4e935e9b))
- *(cluster)* Make `Cluster::from_str` expand worktrees - ([de30c3b](https://github.com/awkless/ocd/commit/de30c3bdc5d7062764fb2abd5afa0b6f6053b097))
- *(cluster)* Add `Cluster::remove_node` - ([2906109](https://github.com/awkless/ocd/commit/29061095c657373f87e45407ae3efd2f7e869024))
- *(cluster)* Add `Cluster::add_node` - ([88fb053](https://github.com/awkless/ocd/commit/88fb053a7d9d437186ac983099972599245af458))

### Miscellaneous Chores

- *(ci)* Merge reuse compliance with quality checks - ([748517c](https://github.com/awkless/ocd/commit/748517c2f944d10d2b8553adc53d49c516ca3357))
- *(cliff)* Use tag for initial version 0.1.0 - ([47fe597](https://github.com/awkless/ocd/commit/47fe597a07561c4d7a3daf3e124e2f6cb363de47))
- *(rustfmt)* Use max heuristics - ([5343039](https://github.com/awkless/ocd/commit/534303977f8768136a5a816f7b18697f2315c3aa))

### Refactoring

- *(clippy)* Simplify code on pedantic settings - ([39ccd2a](https://github.com/awkless/ocd/commit/39ccd2ae90f84946179b0578314f7bec0b90cfe6))
- *(cluster)* Extract root table elements directly - ([b99b839](https://github.com/awkless/ocd/commit/b99b8396a500847b2e1eac3189a458c0ad107eda))
## [0.1.0](https://github.com/awkless/ocd/tree/v0.1.0) - 2025-03-21

### Documentation

- *(changelog)* Document version 0.1.0 - ([211e04a](https://github.com/awkless/ocd/commit/211e04ac980a3948a478bc7e637716e9d096421f))
- *(contrib)* Make commit style more clear - ([aaa7d41](https://github.com/awkless/ocd/commit/aaa7d41e7f8428d50de9ff77ea1874d430b3f30b))
- Add MIT, Apache-2.0, and CC0-1.0 licenses - ([d7a0cdc](https://github.com/awkless/ocd/commit/d7a0cdc930a042d28a906f524c095f3a752b83b3))
- Add `README.md` file - ([79bf1bc](https://github.com/awkless/ocd/commit/79bf1bcee18398a860228abfd9157937db6850a8))
- Add contribution guidelines - ([495efdc](https://github.com/awkless/ocd/commit/495efdc8ce9b60525b31b350fdc86d9b1ec986f7))
- Add pull request template - ([9049c49](https://github.com/awkless/ocd/commit/9049c49c448a375949986ab6f9ffbd8949ac3f90))
- Add code of conduct - ([d8dab13](https://github.com/awkless/ocd/commit/d8dab13bafb9f21c6fedbd1476cc7a812c664c32))

### Features

- Define `main` - ([d07f12e](https://github.com/awkless/ocd/commit/d07f12e7dba6f3bcedbdde154e1a424712568ce2))

### Miscellaneous Chores

- *(cargo)* Add expected dependencies - ([d6f8dc4](https://github.com/awkless/ocd/commit/d6f8dc4940cd479010671c72ecf93829ec4fda15))
- *(ci)* Add code quality check workflow - ([7a6b95d](https://github.com/awkless/ocd/commit/7a6b95d3debc23f3b92f8052e082e5b6f040611b))
- *(ci)* Add REUSE compliance check - ([701bb79](https://github.com/awkless/ocd/commit/701bb7980c5e81f364b5b259f4860688c654c3c1))
- Ignore `target/` and `patch/` directories - ([e9065f1](https://github.com/awkless/ocd/commit/e9065f172ed8f9044e98f86490d212d2f5b89418))
- Define default textual attributes - ([cb3f037](https://github.com/awkless/ocd/commit/cb3f03721248c965693d171a2a7e4a1fc297aa1b))
- Setup Cargo - ([665eed0](https://github.com/awkless/ocd/commit/665eed0dbdacd39ac75edb2a3d9e05cca82987f6))
- Add deny list - ([e9f9171](https://github.com/awkless/ocd/commit/e9f9171aafcb631695d81e0a5b3d3603f16d2e48))
- Make @awkless main code owner - ([bd5a71d](https://github.com/awkless/ocd/commit/bd5a71d668189752d386f4747b054cbe920658cb))
- Provide bug report template - ([7843bdf](https://github.com/awkless/ocd/commit/7843bdface3f511bd464f289e680659ac3bfb0d2))
- Add feature request template - ([0068cd8](https://github.com/awkless/ocd/commit/0068cd8b97c03ea29177b190accb97d87224f3d4))
- Add spdx tags to issue templates - ([430cbea](https://github.com/awkless/ocd/commit/430cbea8e085204eff1cba10bf0bceedde4e8162))
- Setup rustfmt - ([8ad2f58](https://github.com/awkless/ocd/commit/8ad2f5824ed8a81a2a9ff79b63166ef94aa66760))
- Make clippy lint private code - ([e1ceb93](https://github.com/awkless/ocd/commit/e1ceb931a2778a6e666ebf027ea1622b85f45899))
<!-- generated by git-cliff -->
