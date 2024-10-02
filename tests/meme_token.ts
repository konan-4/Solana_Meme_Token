import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SimplifiedMemeToken } from "../target/types/simplified_meme_token";
import { TOKEN_PROGRAM_ID, createAssociatedTokenAccountInstruction, ASSOCIATED_TOKEN_PROGRAM_ID, getAssociatedTokenAddress, getAccount } from "@solana/spl-token";
import { PublicKey } from "@solana/web3.js";
import { expect } from "chai";

describe("simplified_meme_token", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.SimplifiedMemeToken as Program<SimplifiedMemeToken>;
  const payer = anchor.web3.Keypair.generate();
  const recipient = anchor.web3.Keypair.generate();
  let mint: PublicKey;
  let mintBump: number;
  let payerAta: PublicKey;
  let recipientAta: PublicKey;

  before(async () => {
    console.log("Airdropping SOL to the payer...");
    const signature = await provider.connection.requestAirdrop(payer.publicKey, 2000000000);
    await provider.connection.confirmTransaction(signature);
    console.log("Airdrop confirmed.");

    [mint, mintBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("mint")],
      program.programId
    );
    console.log("Derived mint address:", mint.toBase58());

    payerAta = await getAssociatedTokenAddress(mint, payer.publicKey);
    recipientAta = await getAssociatedTokenAddress(mint, recipient.publicKey);
  });

  it("Initializes the token", async () => {
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

      await provider.connection.confirmTransaction(tx);

      const mintAccount = await provider.connection.getAccountInfo(mint);
      console.log("Mint account after initialization:", mintAccount);
      expect(mintAccount).to.not.be.null;
      console.log("Token initialized successfully.");
    } catch (error) {
      console.error("Error during token initialization:", error);
      throw error;
    }
  });

  it("Mints tokens", async () => {
    const mintAmount = new anchor.BN(100000000); // 1 token with 8 decimals

    console.log("Minting tokens to payer...");
    try {
      const tx = await program.methods
        .mintTokens(mintAmount)
        .accounts({
          mint: mint,
          tokenAccount: payerAta,
          payer: payer.publicKey,
          authority: payer.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([payer])
        .rpc();

      await provider.connection.confirmTransaction(tx);
      console.log("Minting transaction confirmed:", tx);

      let payerBalance = await provider.connection.getTokenAccountBalance(payerAta);
      console.log("Payer's token balance after minting:", payerBalance.value.uiAmount);
      expect(payerBalance.value.uiAmount).to.equal(1);

      // Log additional information for debugging
      console.log("Raw balance data:", payerBalance.value);
      console.log("Amount in smallest units:", payerBalance.value.amount);
      console.log("Decimals:", payerBalance.value.decimals);
    } catch (error) {
      console.error("Error during token minting:", error);
      throw error;
    }
  });
  it("Transfers tokens", async () => {
    const transferAmount = new anchor.BN(50000000); // 0.5 tokens with 8 decimals

    // Create the recipient's Associated Token Account without minting
    console.log("Creating recipient's Associated Token Account...");
    try {
      const createAtaIx = await createAssociatedTokenAccountInstruction(
        payer.publicKey,
        recipientAta,
        recipient.publicKey,
        mint
      );
      const createAtaTx = new anchor.web3.Transaction().add(createAtaIx);
      const createAtaTxSignature = await provider.sendAndConfirm(createAtaTx, [payer]);
      console.log("Recipient's ATA created. Tx signature:", createAtaTxSignature);
    } catch (error) {
      if (error.message.includes("TokenAccountInUse")) {
        console.log("Recipient's ATA already exists. Proceeding with transfer.");
      } else {
        console.error("Error creating recipient's ATA:", error);
        throw error;
      }
    }

    // Proceed with the transfer
    console.log("Transferring tokens to recipient...");
    try {
      const transferTx = await program.methods
        .transferTokens(transferAmount)
        .accounts({
          mint: mint,
          from: payerAta,
          to: recipientAta,
          authority: payer.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([payer])
        .rpc();

      await provider.connection.confirmTransaction(transferTx);
      console.log("Transfer transaction confirmed:", transferTx);

      let payerBalance = await provider.connection.getTokenAccountBalance(payerAta);
      let recipientBalance = await provider.connection.getTokenAccountBalance(recipientAta);

      console.log("Payer's token balance after transfer:", payerBalance.value.uiAmount);
      console.log("Recipient's token balance after transfer:", recipientBalance.value.uiAmount);

      expect(payerBalance.value.uiAmount).to.equal(0.5);
      expect(recipientBalance.value.uiAmount).to.equal(0.5);
    } catch (error) {
      console.error("Error during token transfer:", error);
      throw error;
    }
  });
});
