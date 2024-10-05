mod cpamm;
use crate::cpamm::amm_cpi;
use crate::cpamm::ProxyInitialize;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::native_token::LAMPORTS_PER_SOL;
use anchor_lang::solana_program::program::invoke;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use std::mem::size_of;
declare_id!("4Nd1mZSR4cFVfZ2txTPcm97hYkBaPzds6h2SB1ShbFt1");

#[program]
mod fair_launch {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, params: FairLaunchParams) -> Result<()> {
        let fair_launch = &mut ctx.accounts.fair_launch;
        let clock = Clock::get()?;

        // Set initial values
        fair_launch.authority = ctx.accounts.authority.key();
        fair_launch.total_dispatch = params.total_supply;
        fair_launch.until_slot = clock.slot + params.after_slot;
        fair_launch.soft_top_cap = params.soft_top_cap;
        fair_launch.refund_fee_rate = params.refund_fee_rate;
        fair_launch.started = false;
        fair_launch.total_sol = 0;
        fair_launch.raydium_initialized = false;

        // Initialize the token mint
        let cpi_context = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            token::InitializeMint {
                mint: ctx.accounts.token_mint.to_account_info(),
                rent: ctx.accounts.rent.to_account_info(),
            },
        );
        token::initialize_mint(cpi_context, 9, &fair_launch.key(), Some(&fair_launch.key()))?;

        // Mint total supply to the fair launch account
        let cpi_context = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            token::MintTo {
                mint: ctx.accounts.token_mint.to_account_info(),
                to: ctx.accounts.fair_launch_token_account.to_account_info(),
                authority: fair_launch.to_account_info(),
            },
        );
        token::mint_to(cpi_context, fair_launch.total_dispatch)?;

        // Set other parameters
        fair_launch.name = params.name;
        fair_launch.symbol = params.symbol;
        fair_launch.meta = params.meta;
        fair_launch.refund_fee_to = ctx.accounts.refund_fee_to.key();
        fair_launch.project_owner = ctx.accounts.project_owner.key();

        // Store Raydium parameters for later use
        fair_launch.raydium_params = params.raydium_params;

        Ok(())
    }

    pub fn fund(ctx: Context<Fund>, amount: u64) -> Result<()> {
        let fair_launch = &mut ctx.accounts.fair_launch;
        let user = &ctx.accounts.user;

        // Check if funding is still open
        require!(!fair_launch.started, FairLaunchError::AlreadyStarted);

        // Check minimum funding amount
        require!(amount >= MINIMAL_FUND, FairLaunchError::ValueTooLow);

        // Check funding limit per account
        let user_fund = fair_launch
            .fund_balance_of
            .get(&user.key())
            .copied()
            .unwrap_or(0);
        require!(
            user_fund + amount <= 10 * LAMPORTS_PER_SOL,
            FairLaunchError::FundLimitReached
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

        // Update state
        fair_launch
            .fund_balance_of
            .entry(user.key())
            .and_modify(|balance| *balance += amount)
            .or_insert(amount);
        fair_launch.total_sol += amount;

        emit!(FundEvent {
            user: user.key(),
            amount,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn refund(ctx: Context<Refund>) -> Result<()> {
        let fair_launch = &mut ctx.accounts.fair_launch;
        let user = &ctx.accounts.user;

        // Check if refund is still possible
        require!(!fair_launch.started, FairLaunchError::AlreadyStarted);

        // Get user's fund amount
        let amount = fair_launch
            .fund_balance_of
            .get(&user.key())
            .copied()
            .ok_or(FairLaunchError::NoFund)?;

        // Calculate refund fee
        let fee = (amount * fair_launch.refund_fee_rate as u64) / 10000;
        let refund_amount = amount - fee;

        // Transfer SOL from fair launch account to user
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: fair_launch.to_account_info(),
                to: user.to_account_info(),
            },
        );
        anchor_lang::system_program::transfer(cpi_context, refund_amount)?;

        // Transfer fee if applicable
        if fee > 0 && fair_launch.refund_fee_to != Pubkey::default() {
            let fee_cpi_context = CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                anchor_lang::system_program::Transfer {
                    from: fair_launch.to_account_info(),
                    to: ctx.accounts.refund_fee_to.to_account_info(),
                },
            );
            anchor_lang::system_program::transfer(fee_cpi_context, fee)?;
        }

        // Update state
        fair_launch.fund_balance_of.remove(&user.key());
        fair_launch.total_sol -= amount;

        emit!(RefundEvent {
            user: user.key(),
            amount: refund_amount,
            fee,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn start_trading(ctx: Context<StartTrading>) -> Result<()> {
        let fair_launch = &mut ctx.accounts.fair_launch;
        let clock = Clock::get()?;

        // Check if trading can start
        require!(
            clock.slot >= fair_launch.until_slot,
            FairLaunchError::TradingNotStarted
        );
        require!(!fair_launch.started, FairLaunchError::AlreadyStarted);

        // Mark as started
        fair_launch.started = true;

        // Calculate fees and amounts
        let total_sol = fair_launch.total_sol;
        let fee = (total_sol * LAUNCH_FEE_RATE) / 10000;
        let left = total_sol - fee;

        let total_add = if fair_launch.soft_top_cap > 0 {
            std::cmp::min(fair_launch.soft_top_cap, left)
        } else {
            left
        };

        // Initialize Raydium pool
        let cpi_program = ctx.accounts.raydium_program.to_account_info();

        let cpi_accounts = ProxyInitialize {
            fair_launch: ctx.accounts.fair_launch.clone(), // Account for the fair launch program
            fair_launch_token_account: ctx.accounts.fair_launch_token_account.clone(), // Token account for the fair launch
            amm_open_orders: ctx.accounts.amm_open_orders.clone(), // Open orders account for the AMM
            raydium_pool: ctx.accounts.raydium_pool.clone(),       // Raydium pool account
            raydium_pool_token_account: ctx.accounts.raydium_pool_token_account.clone(), // Token account for the Raydium pool
            amm_id: ctx.accounts.amm_id.clone(), // AMM ID for the Raydium program
            amm_authority: ctx.accounts.amm_authority.clone(), // AMM authority
            lp_mint: ctx.accounts.lp_mint.clone(), // LP token mint account
            coin_mint: ctx.accounts.coin_mint.clone(), // Coin mint account
            pc_mint: ctx.accounts.pc_mint.clone(), // PC mint account
            coin_vault: ctx.accounts.coin_vault.clone(), // Coin vault account
            pc_vault: ctx.accounts.pc_vault.clone(), // PC vault account
            amm_target_orders: ctx.accounts.amm_target_orders.clone(), // Target orders for AMM
            pool_withdraw_queue: ctx.accounts.pool_withdraw_queue.clone(), // Withdraw queue for the pool
            rent: ctx.accounts.rent.clone(),                               // Rent sysvar account
        };

        let cpi_ctx = Context::new(cpi_program, cpi_accounts);
        amm_cpi::proxy_initialize(
            cpi_ctx,
            fair_launch.raydium_params.nonce,
            fair_launch.raydium_params.open_time,
            fair_launch.raydium_params.init_pc_amount,
            fair_launch.raydium_params.init_coin_amount,
        )?;

        fair_launch.raydium_initialized = true;

        // Transfer SOL to Raydium pool
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: fair_launch.to_account_info(),
                to: ctx.accounts.pc_vault.to_account_info(),
            },
        );
        anchor_lang::system_program::transfer(cpi_context, total_add)?;

        // Transfer tokens to Raydium pool
        let cpi_context = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.fair_launch_token_account.to_account_info(),
                to: ctx.accounts.coin_vault.to_account_info(),
                authority: fair_launch.to_account_info(),
            },
        );
        token::transfer(cpi_context, fair_launch.total_dispatch / 2)?;

        // Distribute fees
        let refund_fee = total_sol / 1000; // 0.1%
        let project_fee = (total_sol * 2) / 1000; // 0.2%

        // Transfer refund fee
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: fair_launch.to_account_info(),
                to: ctx.accounts.refund_fee_to.to_account_info(),
            },
        );
        anchor_lang::system_program::transfer(cpi_context, refund_fee)?;

        // Transfer project fee
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: fair_launch.to_account_info(),
                to: ctx.accounts.project_owner.to_account_info(),
            },
        );
        anchor_lang::system_program::transfer(cpi_context, project_fee)?;

        emit!(TradingStartedEvent {
            total_sol,
            total_add,
            fee,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    pub fn mint_token(ctx: Context<MintToken>) -> Result<()> {
        let fair_launch = &ctx.accounts.fair_launch;
        let user = &ctx.accounts.user;

        // Check if trading has started
        require!(fair_launch.started, FairLaunchError::TradingNotStarted);

        // Check if user has already minted
        require!(
            !fair_launch.minted.contains(&user.key()),
            FairLaunchError::AlreadyMinted
        );

        // Calculate mint amount
        let user_fund = fair_launch
            .fund_balance_of
            .get(&user.key())
            .copied()
            .unwrap_or(0);
        let mint_amount = (fair_launch.total_dispatch * user_fund) / (2 * fair_launch.total_sol);

        require!(mint_amount > 0, FairLaunchError::ZeroMintAmount);

        // Call external minting program
        let mint_ix = ctx.accounts.meme_program.mint_instruction();
        let account_metas = vec![
            AccountMeta::new(ctx.accounts.token_mint.key(), false),
            AccountMeta::new(ctx.accounts.user_token_account.key(), false),
            AccountMeta::new_readonly(user.key(), true),
            AccountMeta::new_readonly(ctx.accounts.token_program.key(), false),
        ];

        invoke(
            &mint_ix,
            &[
                ctx.accounts.token_mint.to_account_info(),
                ctx.accounts.user_token_account.to_account_info(),
                user.to_account_info(),
                ctx.accounts.token_program.to_account_info(),
            ],
        )?;

        // Mark user as minted
        fair_launch.minted.insert(user.key());

        emit!(TokenMintedEvent {
            user: user.key(),
            amount: mint_amount,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn claim_extra_sol(ctx: Context<ClaimExtraSol>) -> Result<()> {
        let fair_launch = &mut ctx.accounts.fair_launch;
        let user = &ctx.accounts.user;

        // Check if trading has started
        require!(fair_launch.started, FairLaunchError::TradingNotStarted);

        // Check if user has already claimed
        require!(
            !fair_launch.claimed.contains(&user.key()),
            FairLaunchError::AlreadyClaimed
        );

        // Calculate extra SOL
        let total_sol = fair_launch.total_sol;
        let soft_top_cap = fair_launch.soft_top_cap;

        require!(total_sol > soft_top_cap, FairLaunchError::NoExtraSOLToClaim);

        let user_fund = fair_launch
            .fund_balance_of
            .get(&user.key())
            .copied()
            .ok_or(FairLaunchError::NoFund)?;

        let extra_sol = total_sol.saturating_sub(soft_top_cap);
        let user_claim_amount = (user_fund * extra_sol) / total_sol;

        require!(user_claim_amount > 0, FairLaunchError::ZeroClaimAmount);

        // Transfer extra SOL to user
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: fair_launch.to_account_info(),
                to: user.to_account_info(),
            },
        );
        anchor_lang::system_program::transfer(cpi_context, user_claim_amount)?;

        // Mark user as claimed
        fair_launch.claimed.insert(user.key());

        // Update state
        fair_launch.total_sol -= user_claim_amount;

        emit!(ExtraSOLClaimedEvent {
            user: user.key(),
            amount: user_claim_amount,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct FairLaunchParams {
    pub name: String,
    pub symbol: String,
    pub meta: String,
    pub total_supply: u64,
    pub after_slot: u64,
    pub soft_top_cap: u64,
    pub refund_fee_rate: u16,
    pub raydium_params: RaydiumParams,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct RaydiumParams {
    pub nonce: u8,
    pub open_time: u64,
    pub init_pc_amount: u64,
    pub init_coin_amount: u64,
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = 8 + size_of::<FairLaunch>()
    )]
    pub fair_launch: Account<'info, FairLaunch>,

    #[account(
        init,
        payer = authority,
        mint::decimals = 9,
        mint::authority = fair_launch,
    )]
    pub token_mint: Account<'info, Mint>,

    #[account(
        init,
        payer = authority,
        associated_token::mint = token_mint,
        associated_token::authority = fair_launch,
    )]
    pub fair_launch_token_account: Account<'info, TokenAccount>,

    /// CHECK: This is not dangerous because we don't read or write from this account
    pub refund_fee_to: AccountInfo<'info>,

    /// CHECK: This is not dangerous because we don't read or write from this account
    pub project_owner: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

pub struct StartTrading<'info> {
    pub fair_launch: Account<'info, FairLaunch>, // FairLaunch account
    pub fair_launch_token_account: Account<'info, TokenAccount>, // Token account for the fair launch
    pub raydium_pool: AccountInfo<'info>,                        // Raydium pool account
    pub raydium_pool_token_account: Account<'info, TokenAccount>, // Token account for the Raydium pool
    pub amm_open_orders: Account<'info, OpenOrders>,              // Open orders account for the AMM
    pub amm_id: AccountInfo<'info>,                               // AMM ID for the Raydium program
    pub amm_authority: AccountInfo<'info>,                        // AMM authority
    pub lp_mint: Account<'info, Mint>,                            // LP token mint account
    pub coin_mint: Account<'info, Mint>,                          // Coin mint account
    pub pc_mint: Account<'info, Mint>,                            // PC mint account
    pub coin_vault: Account<'info, TokenAccount>,                 // Coin vault account
    pub pc_vault: Account<'info, TokenAccount>,                   // PC vault account
    pub amm_target_orders: Account<'info, OpenOrders>,            // Target orders for AMM
    pub pool_withdraw_queue: AccountInfo<'info>,                  // Withdraw queue for the pool
    pub rent: Sysvar<'info, Rent>,                                // Rent sysvar account
                                                                  // Include any additional accounts that may be needed
}
#[account]
pub struct FairLaunch {
    pub authority: Pubkey,
    pub total_dispatch: u64,
    pub until_slot: u64,
    pub total_sol: u64,
    pub soft_top_cap: u64,
    pub refund_fee_rate: u16,
    pub started: bool,
    pub name: String,
    pub symbol: String,
    pub meta: String,
    pub refund_fee_to: Pubkey,
    pub project_owner: Pubkey,
    pub fund_balance_of: std::collections::HashMap<Pubkey, u64>,
    pub minted: std::collections::HashSet<Pubkey>, // Add other necessary fields
    pub claimed: std::collections::HashSet<Pubkey>,
    pub raydium_params: RaydiumParams,
    pub raydium_initialized: bool,
}
#[error_code]
pub enum FairLaunchError {
    #[msg("Fair launch has already started")]
    AlreadyStarted,
    #[msg("Funding amount is too low")]
    ValueTooLow,
    #[msg("Fund limit reached for this account")]
    FundLimitReached,
    #[msg("No funds available for refund")]
    NoFund,
    #[msg("Trading has not started yet")]
    TradingNotStarted,
    #[msg("User has already minted tokens")]
    AlreadyMinted,
    #[msg("Mint amount is zero")]
    ZeroMintAmount,
    #[msg("User has already claimed extra SOL")]
    AlreadyClaimed,
    #[msg("No extra SOL to claim")]
    NoExtraSOLToClaim,
    #[msg("Claim amount is zero")]
    ZeroClaimAmount,
    #[msg("Raydium initialization failed")]
    RaydiumInitializationFailed,
}

#[event]
pub struct FundEvent {
    pub user: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}
#[event]
pub struct TradingStartedEvent {
    pub total_sol: u64,
    pub total_add: u64,
    pub fee: u64,
    pub timestamp: i64,
}
#[event]
pub struct RefundEvent {
    pub user: Pubkey,
    pub amount: u64,
    pub fee: u64,
    pub timestamp: i64,
}
#[derive(Accounts)]
pub struct MintToken<'info> {
    #[account(mut)]
    pub fair_launch: Account<'info, FairLaunch>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub token_mint: Account<'info, Mint>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    /// CHECK: This account is the Meme program that handles minting
    pub meme_program: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
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
pub struct Refund<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub fair_launch: Account<'info, FairLaunch>,
    /// CHECK: This account is not dangerous as we only transfer SOL to it
    #[account(mut)]
    pub refund_fee_to: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ClaimExtraSol<'info> {
    #[account(mut)]
    pub fair_launch: Account<'info, FairLaunch>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}
#[event]
pub struct ExtraSOLClaimedEvent {
    pub user: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct TokenMintedEvent {
    pub user: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}
// Constants can be defined here, similar to the Solidity contract
pub const REFUND_COMMAND: u64 = 200_000; // 0.0002 SOL in lamports
pub const CLAIM_COMMAND: u64 = 200_000; // 0.0002 SOL in lamports
pub const START_COMMAND: u64 = 500_000; // 0.0005 SOL in lamports
pub const MINT_COMMAND: u64 = 100_000; // 0.0001 SOL in lamports
pub const MINIMAL_FUND: u64 = 100_000; // 0.0001 SOL in lamports
pub const LAUNCH_FEE_RATE: u64 = 30; // 0.3%

// Implement the remaining logic and account structures
