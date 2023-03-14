const { Solita } = require("@metaplex-foundation/solita");
const { spawn } = require("child_process");

const path = require("path");
const idlDir = __dirname;
const sdkDir = path.join(__dirname, "src", "generated");

const PROGRAM_NAME = "phoenix_v1";

async function main() {
  generateTypeScriptSDK().then(() => {
    console.log("Running prettier on generated files...");
    // Note: prettier is not a dependency of this package, so it must be installed
    // TODO: Add a prettier config file for consistent style
    spawn("prettier", ["--write", sdkDir], { stdio: "inherit" })
      .on("error", (err) => {
        console.error(
          "Failed to lint client files. Try installing prettier (`npm install --save-dev --save-exact prettier`)"
        );
      })
      .on("exit", () => {
        console.log("Finished linting files.");
      });
  });
}

async function generateTypeScriptSDK() {
  console.error("Generating TypeScript SDK to %s", sdkDir);
  const generatedIdlPath = path.join(idlDir, `${PROGRAM_NAME}.json`);
  const idl = require(generatedIdlPath);
  const gen = new Solita(idl, { formatCode: true });
  await gen.renderAndWriteTo(sdkDir);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
