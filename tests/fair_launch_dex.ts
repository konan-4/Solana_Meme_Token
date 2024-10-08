
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SimplifiedFairLaunchDex } from "../target/types/simplified_fair_launch_dex";
import * as spl from "@solana/spl-token";
import { expect } from "chai";

describe("simplified-fair-launch-dex", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.SimplifiedFairLaunchDex as Program<SimplifiedFairLaunchDex>;

  let fairLaunch: anchor.web3.PublicKey;
  let fairLaunchBump: number;
  let mint: anchor.web3.PublicKey;
  let fairLaunchTokenAccount: anchor.web3.PublicKey;
  let dexTokenAccount: anchor.web3.PublicKey;
  const totalSupply = new anchor.BN(1_000_000);
  const duration = new anchor.BN(5); // 5 seconds for testing purposes

  before(async () => {
    [fairLaunch, fairLaunchBump] = await anchor.web3.PublicKey.findProgramAddress(
      [Buffer.from("fair_launch")],
      program.programId
    );
    console.log(fairLaunch);

    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(provider.wallet.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL),
      "confirmed"
    );

    mint = await spl.createMint(
      provider.connection,
      provider.wallet.payer,
      provider.wallet.publicKey,
      null,
      9
    );

    fairLaunchTokenAccount = await spl.getOrCreateAssociatedTokenAccount(
      provider.connection,
      provider.wallet.payer,
      mint,
      fairLaunch, // FairLaunch account as the owner
      true
    ).then(account => account.address);

    // Create DEX Token Account (Receives half of the token supply for trading)
    dexTokenAccount = await spl.getOrCreateAssociatedTokenAccount(
      provider.connection,
      provider.wallet.payer,
      mint,
      provider.wallet.publicKey, // Could use the wallet or a DEX program as the owner
      true
    ).then(account => account.address);

    await spl.mintTo(
      provider.connection,
      provider.wallet.payer,
      mint,
      fairLaunchTokenAccount,
      provider.wallet.payer,
      totalSupply.toNumber()
    );
  });

  it("Initializes the fair launch", async () => {

    await program.methods
      .initialize(totalSupply, duration)
      .accounts({
        authority: provider.wallet.publicKey,
        fairLaunch: fairLaunch,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc()
      .then(() => console.log("FairLaunch initialized successfully"))
      .catch((error) => console.error("Error initializing FairLaunch:", error));

  });


    it("Allows funding", async () => {
      const fundAmount = new anchor.BN(1 * anchor.web3.LAMPORTS_PER_SOL);

      await program.methods
        .fund(fundAmount)
        .accounts({
          user: provider.wallet.publicKey,
          fairLaunch: fairLaunch,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc()
        .then(() => console.log("Funding successful"))
        .catch((error) => {
          console.error("Error during funding:", error);
          throw error; // Re-throw to fail the test if necessary
        });

      const fairLaunchAccount = await program.account.fairLaunch.fetch(fairLaunch);
      expect(fairLaunchAccount.totalSol.eq(fundAmount)).to.be.true;
    });
    it("Starts trading after fair launch period ends", async () => {
      // Wait for the fair launch period to end
      await new Promise(resolve => setTimeout(resolve, (duration.toNumber() + 1) * 1000));

      // Start trading
      await program.methods
        .startTrading()
        .accounts({
          fairLaunch: fairLaunch,
          fairLaunchTokenAccount: fairLaunchTokenAccount,
          dexTokenAccount: dexTokenAccount,
          tokenProgram: spl.TOKEN_PROGRAM_ID,
        })
        .rpc();

      // Fetch balances after trading using spl.getAccount
      const fairLaunchTokenAccountInfo = await spl.getAccount(provider.connection, fairLaunchTokenAccount);
      const dexTokenAccountInfo = await spl.getAccount(provider.connection, dexTokenAccount);

      const fairLaunchTokenBalance = fairLaunchTokenAccountInfo.amount;
      const dexTokenBalance = dexTokenAccountInfo.amount;

      console.log("Fair Launch Token Account Balance after trade:", fairLaunchTokenBalance.toString());
      console.log("DEX Token Account Balance after trade:", dexTokenBalance.toString());

      // Assert the correct amount was transferred
      expect(Number(dexTokenBalance)).to.equal(totalSupply.toNumber() / 2);
    });
  });

