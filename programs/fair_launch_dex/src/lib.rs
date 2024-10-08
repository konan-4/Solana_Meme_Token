use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};
use solana_program::native_token::LAMPORTS_PER_SOL;

declare_id!("Boed7wGmYbwngxJtPdRNZszZdeg778GdpExrjy5rsdZu");

#[program]
pub mod simplified_fair_launch_dex {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, total_supply: u64, duration: i64) -> Result<()> {
        let fair_launch = &mut ctx.accounts.fair_launch;
        fair_launch.authority = ctx.accounts.authority.key();
        fair_launch.total_supply = total_supply;
        fair_launch.end_time = Clock::get()?.unix_timestamp + duration;
        fair_launch.total_sol = 0;
        fair_launch.lp_max_limit = 10 * LAMPORTS_PER_SOL;
        Ok(())
    }

    pub fn fund(ctx: Context<Fund>, amount: u64) -> Result<()> {
        let fair_launch = &mut ctx.accounts.fair_launch;

        // Check if the FairMint period has not ended
        require!(
            Clock::get()?.unix_timestamp < fair_launch.end_time,
            ErrorCode::FairMintEnded
        );

        // Update total SOL raised
        fair_launch.total_sol += amount;

        // Check if total raised exceeds LP Max Limit
        require!(
            fair_launch.total_sol <= fair_launch.lp_max_limit,
            ErrorCode::LpMaxLimitExceeded
        );

        // Transfer SOL from user to fair launch account
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: ctx.accounts.user.to_account_info(),
                to: fair_launch.to_account_info(),
            },
        );

        // Transfer the specified amount of SOL
        anchor_lang::system_program::transfer(cpi_context, amount)?;

        // Add or update the contributor's contribution
        if let Some(position) = fair_launch
            .contributions
            .iter()
            .position(|(addr, _)| *addr == ctx.accounts.user.key())
        {
            // Update existing contribution
            fair_launch.contributions[position].1 += amount;
        } else {
            // Add new contributor
            fair_launch.contributors.push(ctx.accounts.user.key());
            fair_launch
                .contributions
                .push((ctx.accounts.user.key(), amount)); // Add new contribution
        }

        Ok(())
    }

    pub fn start_trading(ctx: Context<StartTrading>) -> Result<()> {
        let fair_launch = &ctx.accounts.fair_launch;

        // Ensure fair launch period has ended before trading
        require!(
            Clock::get()?.unix_timestamp >= fair_launch.end_time,
            ErrorCode::FairMintNotEnded
        );

        // Calculate half of the total supply for DEX transfer
        let tokens_to_dex = fair_launch.total_supply / 2;

        // Check the balance before transfer
        let fair_launch_balance = ctx.accounts.fair_launch_token_account.amount;
        let dex_balance = ctx.accounts.dex_token_account.amount;
        msg!(
            "Before Transfer - Fair Launch Token Balance: {}",
            fair_launch_balance
        );
        msg!("Before Transfer - DEX Token Balance: {}", dex_balance);

        // PDA signing setup
        let seeds = &[b"fair_launch".as_ref(), &[ctx.bumps.fair_launch]];
        let signer = &[&seeds[..]];

        // Execute token transfer
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            token::Transfer {
                from: ctx.accounts.fair_launch_token_account.to_account_info(),
                to: ctx.accounts.dex_token_account.to_account_info(),
                authority: fair_launch.to_account_info(),
            },
            signer,
        );
        token::transfer(cpi_ctx, tokens_to_dex)?;

        // Check the balance after transfer
        let fair_launch_balance_after = ctx.accounts.fair_launch_token_account.amount;
        let dex_balance_after = ctx.accounts.dex_token_account.amount;
        msg!(
            "After Transfer - Fair Launch Token Balance: {}",
            fair_launch_balance_after
        );
        msg!("After Transfer - DEX Token Balance: {}", dex_balance_after);

        Ok(())
    }

    pub fn distribute_tokens(ctx: Context<DistributeTokens>) -> Result<()> {
        let fair_launch = &ctx.accounts.fair_launch;

        // Ensure that the FairMint period has ended
        require!(
            Clock::get()?.unix_timestamp >= fair_launch.end_time,
            ErrorCode::FairMintNotEnded
        );

        // Calculate rent fee (2% of total SOL raised)
        let rent_fee_percentage = 2; // 2%
        let rent_fee = (fair_launch.total_sol * rent_fee_percentage) / 100;

        // Deduct rent fee from total SOL raised
        let remaining_funds = fair_launch.total_sol - rent_fee;

        // Calculate total contributions
        let total_contributions: u64 = fair_launch
            .contributions
            .iter()
            .map(|(_, amount)| *amount)
            .sum();

        // Distribute tokens to contributors based on their contribution ratio
        for (contributor, amount) in &fair_launch.contributions {
            // Calculate the token amount based on contribution ratio
            let token_amount = if total_contributions > 0 {
                (amount * remaining_funds) / total_contributions // Calculate tokens based on remaining funds
            } else {
                0 // Handle case where no contributions were made
            };

            // Mint tokens to the contributor's token account
            let cpi_context = CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::MintTo {
                    mint: ctx.accounts.token_mint.to_account_info(),
                    to: ctx.accounts.token_account.to_account_info(),
                    authority: fair_launch.to_account_info(),
                },
            );

            token::mint_to(cpi_context, token_amount)?;
        }

        // If total SOL raised is less than the LP Max Limit, burn the remaining tokens
        if remaining_funds < fair_launch.lp_max_limit {
            let burn_amount = fair_launch.total_supply
                - (fair_launch.total_supply * remaining_funds) / fair_launch.lp_max_limit;

            // Create the CPI context for burning the tokens
            let cpi_context_burn = CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Burn {
                    mint: ctx.accounts.token_mint.to_account_info(), // The mint account for the tokens
                    from: ctx.accounts.token_account.to_account_info(), // The account holding the tokens to burn
                    authority: ctx.accounts.fair_launch.to_account_info(), // The authority that can burn the tokens
                },
            );

            // Perform the burn operation
            token::burn(cpi_context_burn, burn_amount)?;
        }

        // Transfer the rent fee to a designated account (could be a treasury or another account)
        let rent_fee_account = ctx.accounts.rent_fee_account.to_account_info();
        let cpi_context_transfer = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: fair_launch.to_account_info(),
                to: rent_fee_account,
            },
        );

        // Transfer the rent fee
        anchor_lang::system_program::transfer(cpi_context_transfer, rent_fee)?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = fair_launch_space(),
        seeds = [b"fair_launch"],
        bump
    )]
    pub fair_launch: Account<'info, FairLaunch>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Fund<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub fair_launch: Account<'info, FairLaunch>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct StartTrading<'info> {
    #[account(mut, seeds = [b"fair_launch"], bump)]
    pub fair_launch: Account<'info, FairLaunch>,
    #[account(mut)]
    pub fair_launch_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub dex_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[account]
pub struct FairLaunch {
    pub authority: Pubkey,
    pub total_supply: u64,
    pub end_time: i64,
    pub total_sol: u64,
    pub lp_max_limit: u64,                 // Maximum limit for LP
    pub contributors: Vec<Pubkey>,         // List of contributors
    pub contributions: Vec<(Pubkey, u64)>, // Vector of (contributor address, amount contributed)
}

// Calculate space based on a maximum number of contributors and contributions
const MAX_CONTRIBUTORS: usize = 100;
const MAX_CONTRIBUTIONS: usize = 100;

// Space calculation
pub fn fair_launch_space() -> usize {
    8 +                   // Discriminator
    32 +                  // Pubkey: authority
    8 +                   // u64: total_supply
    8 +                   // i64: end_time
    8 +                   // u64: total_sol
    8 +                   // u64: lp_max_limit
    4 + (32 * MAX_CONTRIBUTORS) +      // Vec<Pubkey>: contributors (4 bytes for length prefix + 32 bytes per Pubkey)
    4 + (32 + 8) * MAX_CONTRIBUTIONS // Vec<(Pubkey, u64)>: contributions (4 bytes for length prefix + 32 bytes for Pubkey + 8 bytes for u64)
}

#[derive(Accounts)]
pub struct DistributeTokens<'info> {
    #[account(mut)]
    pub fair_launch: Account<'info, FairLaunch>,
    pub token_program: Program<'info, Token>,
    pub token_mint: Account<'info, Mint>, // The mint account for the tokens
    #[account(mut)]
    pub token_account: Account<'info, TokenAccount>, // The account to receive the minted tokens
    /// CHECK: This account will receive the rent fee, and we assume it is a valid account
    pub rent_fee_account: AccountInfo<'info>, // Account to receive rent fee
    pub system_program: Program<'info, System>,
}

#[error_code]
pub enum ErrorCode {
    #[msg("FairMint ended")]
    FairMintEnded,
    #[msg("FairMint not ended yet")]
    FairMintNotEnded,
    #[msg("Max limit exceeded")]
    LpMaxLimitExceeded,
}
