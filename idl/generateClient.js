const { Solita } = require("@metaplex-foundation/solita");

const path = require("path");
const idlDir = __dirname;
const sdkDir = path.join(__dirname, "src", "generated");

const PROGRAM_NAME = "phoenix_v1";

async function main() {
  generateTypeScriptSDK().then(() => {
    conoole.log("done");
  });
}

async function generateTypeScriptSDK() {
  console.error("Generating TypeScript SDK to %s", sdkDir);
  const generatedIdlPath = path.join(idlDir, `${PROGRAM_NAME}.json`);
  const idl = require(generatedIdlPath);
  const gen = new Solita(idl, { formatCode: true });
  await gen.renderAndWriteTo(sdkDir);
  console.error("Success!");
  process.exit(0);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
