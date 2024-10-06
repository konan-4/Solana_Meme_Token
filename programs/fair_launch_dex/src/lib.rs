use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount};
use spl_token::instruction::AuthorityType;

declare_id!("Boed7wGmYbwngxJtPdRNZszZdeg778GdpExrjy5rsdZu");
#[program]
pub mod fair_launch_dex {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, params: FairLaunchParams) -> Result<()> {
        let fair_launch = &mut ctx.accounts.fair_launch;

        fair_launch.authority = ctx.accounts.authority.key();
        fair_launch.total_supply = params.total_supply;
        fair_launch.fair_mint_supply = fair_launch.total_supply / 2;
        fair_launch.lp_supply = fair_launch.total_supply / 2;
        fair_launch.end_time = Clock::get()?.unix_timestamp + params.duration;
        fair_launch.lp_max_limit = params.lp_max_limit;
        fair_launch.started = false;
        fair_launch.total_sol = 0;
        fair_launch.bump = ctx.bumps.fair_launch;

        Ok(())
    }

    pub fn fund(ctx: Context<Fund>, amount: u64) -> Result<()> {
        let fair_launch = &mut ctx.accounts.fair_launch;
        let user = &ctx.accounts.user;

        require!(!fair_launch.started, FairLaunchError::AlreadyStarted);
        require!(
            Clock::get()?.unix_timestamp < fair_launch.end_time,
            FairLaunchError::FairMintEnded
        );
        require!(
            amount <= MAX_PARTICIPATION,
            FairLaunchError::ExceedsMaxParticipation
        );

        let user_key = user.key();
        let user_participation = fair_launch
            .participations
            .iter()
            .find(|(pubkey, _)| pubkey == &user_key)
            .map(|(_, amount)| *amount)
            .unwrap_or(0);

        require!(
            user_participation + amount <= MAX_PARTICIPATION,
            FairLaunchError::ExceedsMaxParticipation
        );

        // Transfer SOL from user to fair launch account
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: user.to_account_info(),
                to: fair_launch.to_account_info(),
            },
        );
        anchor_lang::system_program::transfer(cpi_context, amount)?;

        // Update or insert user participation
        if let Some(entry) = fair_launch
            .participations
            .iter_mut()
            .find(|(pubkey, _)| pubkey == &user_key)
        {
            entry.1 += amount;
        } else {
            fair_launch.participations.push((user_key, amount));
        }
        fair_launch.total_sol += amount;

        Ok(())
    }

    pub fn refund(ctx: Context<Refund>) -> Result<()> {
        let fair_launch = &mut ctx.accounts.fair_launch;
        let user = &ctx.accounts.user;

        require!(!fair_launch.started, FairLaunchError::AlreadyStarted);

        let user_key = user.key();
        let (index, user_participation) = fair_launch
            .participations
            .iter()
            .enumerate()
            .find(|(_, (pubkey, _))| pubkey == &user_key)
            .map(|(i, (_, amount))| (i, *amount))
            .ok_or(FairLaunchError::NothingToRefund)?;

        // Transfer SOL back to user
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: fair_launch.to_account_info(),
                to: user.to_account_info(),
            },
        );
        anchor_lang::system_program::transfer(cpi_context, user_participation)?;

        fair_launch.total_sol -= user_participation;
        fair_launch.participations.remove(index);

        Ok(())
    }

    pub fn start_trading(ctx: Context<StartTrading>) -> Result<()> {
        let fair_launch = &mut ctx.accounts.fair_launch;
        let dex = &mut ctx.accounts.dex;

        require!(!fair_launch.started, FairLaunchError::AlreadyStarted);
        require!(
            Clock::get()?.unix_timestamp >= fair_launch.end_time,
            FairLaunchError::FairMintNotEnded
        );

        fair_launch.started = true;

        // Add liquidity to DEX
        let lp_sol = std::cmp::min(fair_launch.total_sol, fair_launch.lp_max_limit);
        let lp_tokens = fair_launch.lp_supply;

        dex.sol_reserve = lp_sol;
        dex.token_reserve = lp_tokens;

        // Transfer tokens to DEX
        let cpi_context = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            token::Transfer {
                from: ctx.accounts.fair_launch_token_account.to_account_info(),
                to: ctx.accounts.dex_token_account.to_account_info(),
                authority: fair_launch.to_account_info(),
            },
        );
        token::transfer(cpi_context, lp_tokens)?;

        // Transfer SOL to DEX
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: fair_launch.to_account_info(),
                to: ctx.accounts.dex.to_account_info(),
            },
        );
        anchor_lang::system_program::transfer(cpi_context, lp_sol)?;

        // Burn remaining tokens if any
        if fair_launch.total_sol < fair_launch.lp_max_limit {
            let burn_amount = fair_launch.lp_supply - lp_tokens;
            let cpi_context = CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Burn {
                    mint: ctx.accounts.token_mint.to_account_info(),
                    from: ctx.accounts.fair_launch_token_account.to_account_info(),
                    authority: fair_launch.to_account_info(),
                },
            );
            token::burn(cpi_context, burn_amount)?;
        }

        Ok(())
    }

    pub fn mint_token(ctx: Context<MintToken>) -> Result<()> {
        let fair_launch = &ctx.accounts.fair_launch;
        let user = &ctx.accounts.user;

        require!(fair_launch.started, FairLaunchError::TradingNotStarted);

        let user_key = user.key();
        let user_participation = fair_launch
            .participations
            .iter()
            .find(|(pubkey, _)| pubkey == &user_key)
            .map(|(_, amount)| *amount)
            .ok_or(FairLaunchError::NothingToClaim)?;

        let token_amount =
            (fair_launch.fair_mint_supply * user_participation) / fair_launch.total_sol;

        // Transfer tokens to user
        let cpi_context = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            token::Transfer {
                from: ctx.accounts.fair_launch_token_account.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: fair_launch.to_account_info(),
            },
        );
        token::transfer(cpi_context, token_amount)?;

        Ok(())
    }

    pub fn claim_extra_sol(ctx: Context<ClaimExtraSol>) -> Result<()> {
        let fair_launch = &mut ctx.accounts.fair_launch;
        let user = &ctx.accounts.user;

        require!(fair_launch.started, FairLaunchError::TradingNotStarted);

        let user_key = user.key();
        let user_participation = fair_launch
            .participations
            .iter()
            .find(|(pubkey, _)| pubkey == &user_key)
            .map(|(_, amount)| *amount)
            .ok_or(FairLaunchError::NothingToClaim)?;

        let sol_refund = if fair_launch.total_sol > fair_launch.lp_max_limit {
            (user_participation * (fair_launch.total_sol - fair_launch.lp_max_limit))
                / fair_launch.total_sol
        } else {
            0
        };

        require!(sol_refund > 0, FairLaunchError::NoExtraSolToClaim);

        // Refund excess SOL
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: fair_launch.to_account_info(),
                to: user.to_account_info(),
            },
        );
        anchor_lang::system_program::transfer(cpi_context, sol_refund)?;

        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct FairLaunchParams {
    pub total_supply: u64,
    pub duration: i64,
    pub lp_max_limit: u64,
}

#[account]
#[derive(Default)]
pub struct FairLaunch {
    pub authority: Pubkey,
    pub total_supply: u64,
    pub fair_mint_supply: u64,
    pub lp_supply: u64,
    pub end_time: i64,
    pub lp_max_limit: u64,
    pub started: bool,
    pub total_sol: u64,
    pub bump: u8,
    pub participations: Vec<(Pubkey, u64)>,
}

#[account]
#[derive(Default)]
pub struct Dex {
    pub sol_reserve: u64,
    pub token_reserve: u64,
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8 + 8 + 8 + 8 + 1 + 8 + 1 + 4 + (10 * (32 + 8)), // Adjusted space calculation
        seeds = [b"fair_launch".as_ref()],
        bump
    )]
    pub fair_launch: Account<'info, FairLaunch>,
    #[account(mut)]
    pub token_mint: Account<'info, Mint>,
    #[account(
        init,
        payer = authority,
        associated_token::mint = token_mint,
        associated_token::authority = fair_launch
    )]
    pub fair_launch_token_account: Account<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
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
pub struct Refund<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub fair_launch: Account<'info, FairLaunch>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct StartTrading<'info> {
    #[account(mut)]
    pub fair_launch: Account<'info, FairLaunch>,
    #[account(mut)]
    pub fair_launch_token_account: Account<'info, TokenAccount>,
    #[account(init, payer = fair_launch, space = 8 + 8 + 8)]
    pub dex: Account<'info, Dex>,
    #[account(mut)]
    pub dex_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub token_mint: Account<'info, Mint>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct MintToken<'info> {
    #[account(mut)]
    pub fair_launch: Account<'info, FairLaunch>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub fair_launch_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ClaimExtraSol<'info> {
    #[account(mut)]
    pub fair_launch: Account<'info, FairLaunch>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[error_code]
pub enum FairLaunchError {
    #[msg("Already started")]
    AlreadyStarted,
    #[msg("FairMint ended")]
    FairMintEnded,
    #[msg("Exceeds maximum participation limit")]
    ExceedsMaxParticipation,
    #[msg("FairMint not ended yet")]
    FairMintNotEnded,
    #[msg("Trading not started")]
    TradingNotStarted,
    #[msg("Nothing to claim")]
    NothingToClaim,
    #[msg("Nothing to refund")]
    NothingToRefund,
    #[msg("No extra SOL to claim")]
    NoExtraSolToClaim,
    #[msg("Invalid PDA")]
    InvalidPda,
}

// Constants
pub const MAX_PARTICIPATION: u64 = 10 * 10u64.pow(9); // 10 SOL
