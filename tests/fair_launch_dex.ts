import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { FairLaunchDex } from "../target/types/fair_launch_dex";
import {
  PublicKey,
  Keypair,
  SystemProgram,
  LAMPORTS_PER_SOL
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createMint,
  getAssociatedTokenAddress
} from "@solana/spl-token";

describe("Fair Launch DEX Initialization", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.FairLaunchDex as Program<FairLaunchDex>;
  const authority = provider.wallet.publicKey;

  let tokenMint: PublicKey;
  let fairLaunchPDA: PublicKey;
  let fairLaunchTokenAccount: PublicKey;

  it("Initializes the Fair Launch DEX", async () => {
    // Create a new token mint
    const mintKeypair = Keypair.generate();
    tokenMint = await createMint(
      provider.connection,
      provider.wallet.payer,
      authority,
      null,
      9,
      mintKeypair
    );

    // Derive the Fair Launch PDA
    [fairLaunchPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("fair_launch")],
      program.programId
    );

    // Derive the associated token account for the Fair Launch
    fairLaunchTokenAccount = await getAssociatedTokenAddress(
      tokenMint,
      fairLaunchPDA,
      true // allowOwnerOffCurve
    );

    // Set up the Fair Launch parameters
    const fairLaunchParams = {
      totalSupply: new anchor.BN(1_000_000_000), // 1 billion tokens
      duration: new anchor.BN(7 * 24 * 60 * 60), // 7 days in seconds
      lpMaxLimit: new anchor.BN(100_000 * LAMPORTS_PER_SOL), // 100,000 SOL
    };

    try {
      const tx = await program.methods
        .initialize(fairLaunchParams)
        .accounts({
          authority,
          fairLaunch: fairLaunchPDA,
          tokenMint,
          fairLaunchTokenAccount,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        })
        .rpc();

      console.log("Fair Launch DEX initialized successfully!");
      console.log("Transaction signature:", tx);
      console.log("Token Mint:", tokenMint.toBase58());
      console.log("Fair Launch PDA:", fairLaunchPDA.toBase58());
      console.log("Fair Launch Token Account:", fairLaunchTokenAccount.toBase58());
    } catch (error) {
      console.error("Error initializing Fair Launch DEX:");
      if (error instanceof anchor.AnchorError) {
        console.error("Error code:", error.error.errorCode.code);
        console.error("Error message:", error.error.errorMessage);
        console.error("Error logs:", error.logs);
      } else {
        console.error(error);
      }
      throw error;
    }
  });
  it("Funds the Fair Launch", async () => {
    const fairLaunchParams = {
      totalSupply: new anchor.BN(1_000_000_000),
      duration: new anchor.BN(7 * 24 * 60 * 60),
      lpMaxLimit: new anchor.BN(100_000 * LAMPORTS_PER_SOL),
    };

    await program.methods
      .initialize(fairLaunchParams)
      .accounts({
        authority,
        fairLaunch: fairLaunchPDA,
        tokenMint,
        fairLaunchTokenAccount,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();
    const user = Keypair.generate();
    const fundAmount = new anchor.BN(1 * LAMPORTS_PER_SOL); // Fund 1 SOL

    // Airdrop some SOL to the user for funding
    const airdropSignature = await provider.connection.requestAirdrop(
      user.publicKey,
      2 * LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(airdropSignature);

    try {
      const tx = await program.methods
        .fund(fundAmount)
        .accounts({
          user: user.publicKey,
          fairLaunch: fairLaunchPDA,
          systemProgram: SystemProgram.programId,
        })
        .signers([user])
        .rpc();

      console.log("Funding transaction signature:", tx);

      // Fetch the updated Fair Launch account
      const fairLaunchAccount = await program.account.fairLaunch.fetch(fairLaunchPDA);

      // Assertions
      expect(fairLaunchAccount.totalSol.toNumber()).to.equal(fundAmount.toNumber());
      expect(fairLaunchAccount.participations).to.have.lengthOf(1);
      expect(fairLaunchAccount.participations[0][0].toBase58()).to.equal(user.publicKey.toBase58());
      expect(fairLaunchAccount.participations[0][1].toNumber()).to.equal(fundAmount.toNumber());

      // Check the SOL balance of the Fair Launch account
      const fairLaunchBalance = await provider.connection.getBalance(fairLaunchPDA);
      expect(fairLaunchBalance).to.equal(fundAmount.toNumber());

    } catch (error) {
      console.error("Error funding Fair Launch:");
      if (error instanceof anchor.AnchorError) {
        console.error("Error code:", error.error.errorCode.code);
        console.error("Error message:", error.error.errorMessage);
        console.error("Error logs:", error.logs);
      } else {
        console.error(error);
      }
      throw error;
    }
  });
});
