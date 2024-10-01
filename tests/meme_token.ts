import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SimplifiedMemeToken } from "../target/types/simplified_meme_token";
import { TOKEN_PROGRAM_ID, ASSOCIATED_TOKEN_PROGRAM_ID, getAssociatedTokenAddress } from "@solana/spl-token";
import { PublicKey, Transaction, sendAndConfirmTransaction } from "@solana/web3.js";
import { expect } from "chai";

describe("simplified_meme_token", () => {
  // Configure the client to use the local cluster
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.SimplifiedMemeToken as Program<SimplifiedMemeToken>;
  const payer = anchor.web3.Keypair.generate();
  let mint: PublicKey;
  let mintBump: number;

  before(async () => {
    console.log("Airdropping SOL to the payer...");
    const signature = await provider.connection.requestAirdrop(payer.publicKey, 1000000000);
    await provider.connection.confirmTransaction(signature);
    console.log("Airdrop confirmed.");

    // Derive the mint address
    [mint, mintBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("mint")],
      program.programId
    );
    console.log("Derived mint address:", mint.toBase58());
  });

  async function cleanupAccount(accountPubkey: PublicKey) {
    try {
      const accountInfo = await provider.connection.getAccountInfo(accountPubkey);
      if (accountInfo !== null) {
        console.log(`Account ${accountPubkey.toBase58()} exists. Attempting to close...`);
        const transaction = new Transaction().add(
          anchor.web3.SystemProgram.assign({
            accountPubkey: accountPubkey,
            programId: anchor.web3.SystemProgram.programId,
          })
        );
        await sendAndConfirmTransaction(provider.connection, transaction, [payer]);
        console.log(`Account ${accountPubkey.toBase58()} closed.`);
      } else {
        console.log(`Account ${accountPubkey.toBase58()} does not exist. No cleanup needed.`);
      }
    } catch (error) {
      console.error(`Error during cleanup of ${accountPubkey.toBase58()}:`, error);
    }
  }

  it("Initializes the token", async () => {
    // Cleanup before initializing
    await cleanupAccount(mint);

    console.log("Attempting to initialize token...");
    try {
      const tx = await program.methods
        .initToken(8) // 8 decimals
        .accounts({
          mint: mint,
          payer: payer.publicKey,
          authority: payer.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([payer])
        .rpc();

      console.log("Token initialization transaction sent:", tx);

      // Wait for confirmation
      const confirmation = await provider.connection.confirmTransaction(tx);
      console.log("Transaction confirmed:", confirmation);

      // Fetch and log the mint account info
      const mintAccount = await provider.connection.getAccountInfo(mint);
      console.log("Mint account after initialization:", mintAccount);

      expect(mintAccount).to.not.be.null;
      console.log("Token initialized successfully.");
    } catch (error) {
      console.error("Error during token initialization:", error);
      throw error; // Re-throw the error to fail the test
    }
  });

  // ... rest of the tests ...
});
