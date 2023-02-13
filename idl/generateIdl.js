const path = require("path");
const fs = require("fs");
const programDir = path.join(__dirname, "..", "program");
const idlDir = __dirname;
const rootDir = path.join(__dirname, ".crates");
const cargoToml = path.join(programDir, "Cargo.toml");

const PROGRAM_NAME = "phoenix";

const {
  rustbinMatch,
  confirmAutoMessageConsole,
} = require("@metaplex-foundation/rustbin");
const { spawn } = require("child_process");
const rustbinConfig = {
  rootDir,
  binaryName: "shank",
  binaryCrateName: "shank-cli",
  libName: "shank",
  dryRun: false,
  cargoToml,
};

async function main() {
  const { fullPathToBinary: shankExecutable } = await rustbinMatch(
    rustbinConfig,
    confirmAutoMessageConsole
  );
  const shank = spawn(shankExecutable, [
    "idl",
    "--out-dir",
    idlDir,
    "--crate-root",
    programDir,
  ])
    .on("error", (err) => {
      console.error(err);
      if (err.code === "ENOENT") {
        console.error(
          "Ensure that `shank` is installed and in your path, see:\n  https://github.com/metaplex-foundation/shank\n"
        );
      }
      process.exit(1);
    })
    .on("exit", () => {
      mutateIdl();
    });

  shank.stdout.on("data", (buf) => console.log(buf.toString("utf8")));
  shank.stderr.on("data", (buf) => console.error(buf.toString("utf8")));
}

function mutateIdl() {
  console.error("Mutating IDL");
  const generatedIdlPath = path.join(idlDir, `${PROGRAM_NAME}.json`);
  let idl = require(generatedIdlPath);
  for (const instruction of idl.instructions) {
    if (
      instruction.name === "ReduceOrder" ||
      instruction.name === "ReduceOrderWithFreeFunds"
    ) {
      instruction.args.push({
        name: "params",
        type: {
          defined: "ReduceOrderParams",
        },
      });
    }
    if (
      instruction.name === "CancelMulitpleOrdersById" ||
      instruction.name === "CancelMulitpleOrdersByIdWithFreeFunds"
    ) {
      instruction.args.push({
        name: "params",
        type: {
          defined: "CancelMulitpleOrdersByIdParams",
        },
      });
    }
    if (
      instruction.name === "PlaceLimitOrder" ||
      instruction.name === "PlaceLimitOrderWithFreeFunds" ||
      instruction.name === "Swap" ||
      instruction.name === "SwapWithFreeFunds"
    ) {
      instruction.args.push({
        name: "orderPacket",
        type: {
          defined: "OrderPacket",
        },
      });
    }
    if (
      instruction.name === "PlaceMultiplePostOnlyOrders" ||
      instruction.name === "PlaceMultiplePostOnlyOrdersWithFreeFunds"
    ) {
      instruction.args.push({
        name: "multipleOrderPacket",
        type: {
          defined: "MultipleOrderPacket",
        },
      });
    }
    if (
      instruction.name === "CancelUpTo" ||
      instruction.name === "CancelUpToWithFreeFunds" ||
      instruction.name === "ForceCancelOrders"
    ) {
      instruction.args.push({
        name: "params",
        type: {
          defined: "CancelUpToParams",
        },
      });
    }
    if (instruction.name === "DepositFunds") {
      instruction.args.push({
        name: "depositFundsParams",
        type: {
          defined: "DepositParams",
        },
      });
    }
    if (instruction.name === "WithdrawFunds") {
      instruction.args.push({
        name: "withdrawFundsParams",
        type: {
          defined: "WithdrawParams",
        },
      });
    }
    if (instruction.name === "ChangeSeatStatus") {
      instruction.args.push({
        name: "approvalStatus",
        type: {
          defined: "SeatApprovalStatus",
        },
      });
    }
    if (instruction.name === "ChangeMarketStatus") {
      instruction.args.push({
        name: "marketStatus",
        type: {
          defined: "MarketStatus",
        },
      });
    }
    if (instruction.name === "InitializeMarket") {
      instruction.args.push({
        name: "initializeParams",
        type: {
          defined: "InitializeParams",
        },
      });
    }
    if (instruction.name === "NameSuccessor") {
      instruction.args.push({
        name: "successor",
        type: "publicKey",
      });
    }
  }
  fs.writeFileSync(generatedIdlPath, JSON.stringify(idl, null, 2));
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
