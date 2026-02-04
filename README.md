# phoenix

Phoenix Legacy is an on-chain orderbook that operates without a crank.

### Documentation

Documentation and instructions on how to interact with the program are available on [GitBook](https://ellipsis-labs.gitbook.io/phoenix-dex/tRIkEFlLUzWK9uKO3W2V/getting-started/phoenix-overview).

### Licensing

The primary license for Phoenix Legacy is the Business Source License 1.1 (`BUSL-1.1`), which can be found at [`LICENSE`](https://github.com/Ellipsis-Labs/phoenix-v1/blob/master/LICENSE).

### Audits

Phoenix Legacy has been audited by OtterSec. The audit report can be found at [audits/OtterSec.pdf](https://github.com/Ellipsis-Labs/phoenix-v1/blob/master/audits/OtterSec.pdf).

### Bug Bounty

Information on the bug bounty program for Phoenix Legacy can be found at [SECURITY.md](https://github.com/Ellipsis-Labs/phoenix-v1/blob/master/SECURITY.md).

### Build Verification

You can use [Solana Verify CLI](https://github.com/Ellipsis-Labs/solana-verifiable-build) to verify that the program deployed at `PhoeNiXZ8ByJGLkxNfZRnkUfjvmuYqLR89jjFHGqdXY` matches the code in this repository. After installing the CLI, run:

```
solana-verify verify-from-repo -um --program-id PhoeNiXZ8ByJGLkxNfZRnkUfjvmuYqLR89jjFHGqdXY https://github.com/Ellipsis-Labs/phoenix-v1
```

This may take awhile as it builds the program inside Docker, then verifies that the build hash matches the deployed program hash. The verification process is much faster on a non-ARM machine.

### Building and Testing Locally

To build the contract, run:

```
./build.sh
```

To run the tests, run:

```
./test.sh
```
