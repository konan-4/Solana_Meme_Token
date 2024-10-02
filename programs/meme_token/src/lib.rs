use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount, Transfer},
};

declare_id!("6oMv3v3uUWGM4NwgHFdgLQXACh9jzyrkKBhJvdF31EzL");

#[program]
pub mod simplified_meme_token {
    use super::*;

    pub fn init_token(ctx: Context<InitToken>, decimals: u8) -> Result<()> {
        msg!(
            "Token mint created successfully with {} decimals.",
            decimals
        );
        Ok(())
    }

    pub fn mint_tokens(ctx: Context<MintTokens>, amount: u64) -> Result<()> {
        anchor_spl::token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                anchor_spl::token::MintTo {
                    mint: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.token_account.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
                &[&[b"mint".as_ref(), &[ctx.bumps.mint]]],
            ),
            amount,
        )?;
        msg!("Minted {} tokens to the token account.", amount);
        Ok(())
    }

    pub fn transfer_tokens(ctx: Context<TransferTokens>, amount: u64) -> Result<()> {
        anchor_spl::token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.from.to_account_info(),
                    to: ctx.accounts.to.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
            amount,
        )?;
        msg!(
            "Transferred {} tokens from {} to {}.",
            amount,
            ctx.accounts.from.key(),
            ctx.accounts.to.key()
        );
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(decimals: u8)]
pub struct InitToken<'info> {
    #[account(
        init_if_needed,
        seeds = [b"mint"],
        bump,
        payer = payer,
        mint::decimals = decimals,
        mint::authority = authority.key(),
    )]
    pub mint: Account<'info, Mint>,
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: This is the authority that can mint tokens
    #[account(mut)]
    pub authority: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct MintTokens<'info> {
    #[account(
        mut,
        seeds = [b"mint"],
        bump,
    )]
    pub mint: Account<'info, Mint>,
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = payer,
    )]
    pub token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: This is the authority that can mint tokens
    #[account(mut)]
    pub authority: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct TransferTokens<'info> {
    pub mint: Account<'info, Mint>,
    #[account(mut)]
    pub from: Account<'info, TokenAccount>,
    #[account(mut)]
    pub to: Account<'info, TokenAccount>,
    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
}
