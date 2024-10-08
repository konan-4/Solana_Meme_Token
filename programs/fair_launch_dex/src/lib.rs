use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

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
        Ok(())
    }

    pub fn fund(ctx: Context<Fund>, amount: u64) -> Result<()> {
        let fair_launch = &mut ctx.accounts.fair_launch;
        require!(
            Clock::get()?.unix_timestamp < fair_launch.end_time,
            ErrorCode::FairMintEnded
        );

        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: ctx.accounts.user.to_account_info(),
                to: fair_launch.to_account_info(),
            },
        );
        anchor_lang::system_program::transfer(cpi_context, amount)?;

        fair_launch.total_sol += amount;
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
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8 + 8,
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
}

#[error_code]
pub enum ErrorCode {
    #[msg("FairMint ended")]
    FairMintEnded,
    #[msg("FairMint not ended yet")]
    FairMintNotEnded,
}
