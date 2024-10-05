use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke,
};
use anchor_spl::token::{Mint, TokenAccount};

#[program]
pub mod amm_cpi {
    use super::*;

    // Initialize AMM (e.g., Raydium) via CPI proxy
    pub fn proxy_initialize(
        ctx: Context<ProxyInitialize>,
        nonce: u8,
        open_time: u64,
        init_pc_amount: u64,
        init_coin_amount: u64,
    ) -> Result<()> {
        let ix_data = vec![nonce];
        ix_data.extend_from_slice(&open_time.to_le_bytes());
        ix_data.extend_from_slice(&init_pc_amount.to_le_bytes());
        ix_data.extend_from_slice(&init_coin_amount.to_le_bytes());

        // Create the CPI instruction
        let instruction = Instruction {
            program_id: ctx.accounts.amm_id.key(),
            accounts: vec![
                AccountMeta::new(ctx.accounts.amm_id.key(), false),
                AccountMeta::new(ctx.accounts.amm_authority.key(), false),
                AccountMeta::new(ctx.accounts.amm_open_orders.key(), false),
                AccountMeta::new(ctx.accounts.lp_mint.key(), false),
                AccountMeta::new(ctx.accounts.coin_mint.key(), false),
                AccountMeta::new(ctx.accounts.pc_mint.key(), false),
                AccountMeta::new(ctx.accounts.coin_vault.key(), false),
                AccountMeta::new(ctx.accounts.pc_vault.key(), false),
                AccountMeta::new(ctx.accounts.amm_target_orders.key(), false),
                AccountMeta::new(ctx.accounts.pool_withdraw_queue.key(), false),
                AccountMeta::new_readonly(ctx.accounts.token_program.key(), false),
                AccountMeta::new_readonly(ctx.accounts.system_program.key(), false),
                AccountMeta::new_readonly(ctx.accounts.rent.key(), false),
            ],
            data: ix_data,
        };

        // Perform CPI invocation to AMM
        invoke(
            &instruction,
            &[
                ctx.accounts.amm_id.to_account_info(),
                ctx.accounts.amm_authority.to_account_info(),
                ctx.accounts.amm_open_orders.to_account_info(),
                ctx.accounts.lp_mint.to_account_info(),
                ctx.accounts.coin_mint.to_account_info(),
                ctx.accounts.pc_mint.to_account_info(),
                ctx.accounts.coin_vault.to_account_info(),
                ctx.accounts.pc_vault.to_account_info(),
                ctx.accounts.amm_target_orders.to_account_info(),
                ctx.accounts.pool_withdraw_queue.to_account_info(),
                ctx.accounts.token_program.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
                ctx.accounts.rent.to_account_info(),
            ],
        )?;
        Ok(())
    }
}

#[account]
pub struct ProxyInitialize<'info> {
    pub fair_launch: Account<'info, FairLaunch>, // Account for the fair launch program
    pub fair_launch_token_account: Account<'info, TokenAccount>, // Token account for the fair launch
    pub amm_open_orders: Account<'info, OpenOrders>,             // Open orders account for the AMM
    pub raydium_pool: AccountInfo<'info>,                        // Raydium pool account
    pub raydium_pool_token_account: Account<'info, TokenAccount>, // Token account for the Raydium pool
    pub amm_id: Pubkey,                                           // AMM ID for the Raydium program
    pub amm_authority: Pubkey,                                    // AMM authority
    pub lp_mint: Account<'info, Mint>,                            // LP token mint account
    pub coin_mint: Account<'info, Mint>,                          // Coin mint account
    pub pc_mint: Account<'info, Mint>,                            // PC mint account
    pub coin_vault: Account<'info, TokenAccount>,                 // Coin vault account
    pub pc_vault: Account<'info, TokenAccount>,                   // PC vault account
    pub amm_target_orders: Account<'info, OpenOrders>,            // Target orders for AMM
    pub pool_withdraw_queue: AccountInfo<'info>,                  // Withdraw queue for the pool
    pub rent: Sysvar<'info, Rent>,                                // Rent sysvar account
}

// This structure initializes AMM parameters, such as nonce and the amount of coins.
#[derive(AnchorDeserialize, AnchorSerialize)]
pub struct AmmParams {
    pub nonce: u8,             // Nonce value for AMM initialization
    pub open_time: u64,        // Time when the AMM will open
    pub init_pc_amount: u64,   // Initial pair coin amount (e.g., USDC)
    pub init_coin_amount: u64, // Initial coin amount (e.g., SOL)
}

impl ProxyInitialize<'_> {
    // Helper function to ensure that the correct AMM authority is being used.
    pub fn validate_authority(&self, expected_authority: &Pubkey) -> Result<()> {
        if &self.amm_authority.key() != expected_authority {
            return Err(ErrorCode::InvalidAuthority.into());
        }
        Ok(())
    }
}

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid AMM Authority!")]
    InvalidAuthority,
}
