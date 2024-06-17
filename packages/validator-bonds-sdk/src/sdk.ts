import * as generated from '../generated/validator_bonds'
import {
  Program as AnchorProgram,
  IdlAccounts,
  IdlEvents,
  AnchorProvider,
  Program,
  parseIdlErrors,
  Provider,
  Wallet,
  IdlTypes,
} from '@coral-xyz/anchor'
import { Wallet as AnchorWalletInterface } from '@coral-xyz/anchor/dist/cjs/provider'
import {
  ConfirmOptions,
  Connection,
  EpochInfo,
  Keypair,
  PublicKey,
} from '@solana/web3.js'
import BN from 'bn.js'

export const MARINADE_CONFIG_ADDRESS = new PublicKey(
  'vbMaRfmTCg92HWGzmd53APkMNpPnGVGZTUHwUJQkXAU'
)

export const ValidatorBondsIDL = generated.IDL

export const VALIDATOR_BONDS_PROGRAM_ID = new PublicKey(
  JSON.parse(generated.IDL.constants.find(x => x.name === 'PROGRAM_ID')!.value)
)

export type ValidatorBonds = generated.ValidatorBonds
export type ValidatorBondsProgram = AnchorProgram<ValidatorBonds>

// --- ACCOUNTS ---
export type Config = IdlAccounts<ValidatorBonds>['config']
export type Bond = IdlAccounts<ValidatorBonds>['bond']
export type SettlementClaims = IdlAccounts<ValidatorBonds>['settlementClaims']
export type Settlement = IdlAccounts<ValidatorBonds>['settlement']
export type WithdrawRequest = IdlAccounts<ValidatorBonds>['withdrawRequest']

// --- TYPES ---
export type InitConfigArgs = IdlTypes<ValidatorBonds>['InitConfigArgs']
export type ConfigureConfigArgs =
  IdlTypes<ValidatorBonds>['ConfigureConfigArgs']
export type InitBondArgs = IdlTypes<ValidatorBonds>['InitBondArgs']

// --- CONSTANTS ---
function seedFromConstants(seedName: string): Uint8Array {
  const constant = generated.IDL.constants.find(x => x.name === seedName)
  if (constant === undefined) {
    throw new Error(
      'SDK initialization failure. Validator bonds IDL does not define constant ' +
        constant
    )
  }
  return new Uint8Array(JSON.parse(constant.value))
}
export const BOND_SEED = seedFromConstants('BOND_SEED')
export const BOND_MINT_SEED = seedFromConstants('BOND_MINT_SEED')
export const SETTLEMENT_SEED = seedFromConstants('SETTLEMENT_SEED')
export const WITHDRAW_REQUEST_SEED = seedFromConstants('WITHDRAW_REQUEST_SEED')
export const SETTLEMENT_CLAIMS_SEED = seedFromConstants(
  'SETTLEMENT_CLAIMS_SEED'
)
export const BONDS_WITHDRAWER_AUTHORITY_SEED = seedFromConstants(
  'BONDS_WITHDRAWER_AUTHORITY_SEED'
)
export const SETTLEMENT_STAKER_AUTHORITY_SEED = seedFromConstants(
  'SETTLEMENT_STAKER_AUTHORITY_SEED'
)

// --- EVENTS ---
export const INIT_CONFIG_EVENT = 'InitConfigEvent'
export type InitConfigEvent =
  IdlEvents<ValidatorBonds>[typeof INIT_CONFIG_EVENT]

export const CONFIGURE_CONFIG_EVENT = 'ConfigureConfigEvent'
export type ConfigureConfigEvent =
  IdlEvents<ValidatorBonds>[typeof CONFIGURE_CONFIG_EVENT]

export const INIT_BOND_EVENT = 'InitBondEvent'
export type InitBondEvent = IdlEvents<ValidatorBonds>[typeof INIT_BOND_EVENT]

export const CONFIGURE_BOND_EVENT = 'ConfigureBondEvent'
export type ConfigureBondEvent =
  IdlEvents<ValidatorBonds>[typeof CONFIGURE_BOND_EVENT]

export const CONFIGURE_BOND_WITH_MINT_EVENT = 'ConfigureBondWithMintEvent'
export type ConfigureBondWithMintEvent =
  IdlEvents<ValidatorBonds>[typeof CONFIGURE_BOND_WITH_MINT_EVENT]

export const MINT_BOND_EVENT = 'MintBondEvent'
export type MintBondEvent = IdlEvents<ValidatorBonds>[typeof MINT_BOND_EVENT]

export const FUND_BOND_EVENT = 'FundBondEvent'
export type FundBondEvent = IdlEvents<ValidatorBonds>[typeof FUND_BOND_EVENT]

export const FUND_SETTLEMENT_EVENT = 'FundSettlementEvent'
export type FundSettlementEvent =
  IdlEvents<ValidatorBonds>[typeof FUND_SETTLEMENT_EVENT]

export const CLAIM_SETTLEMENT_EVENT = 'ClaimSettlementEvent'
export type ClaimSettlementEvent =
  IdlEvents<ValidatorBonds>[typeof CLAIM_SETTLEMENT_EVENT]

export const INIT_SETTLEMENT_EVENT = 'InitSettlementEvent'
export type InitSettlementEvent =
  IdlEvents<ValidatorBonds>[typeof INIT_SETTLEMENT_EVENT]

export const CLOSE_SETTLEMENT_EVENT = 'CloseSettlementEvent'
export type CloseSettlementEvent =
  IdlEvents<ValidatorBonds>[typeof CLOSE_SETTLEMENT_EVENT]

export const CANCEL_SETTLEMENT_EVENT = 'CancelSettlementEvent'
export type CancelSettlementEvent =
  IdlEvents<ValidatorBonds>[typeof CANCEL_SETTLEMENT_EVENT]

export const MERGE_STAKE_EVENT = 'MergeStakeEvent'
export type MergeStakeEvent =
  IdlEvents<ValidatorBonds>[typeof MERGE_STAKE_EVENT]

export const RESET_STAKE_EVENT = 'ResetStakeEvent'
export type ResetStakeEvent =
  IdlEvents<ValidatorBonds>[typeof RESET_STAKE_EVENT]

export const WITHDRAW_STAKE_EVENT = 'WithdrawStakeEvent'
export type WithdrawStakeEvent =
  IdlEvents<ValidatorBonds>[typeof WITHDRAW_STAKE_EVENT]

export const INIT_WITHDRAW_REQUEST_EVENT = 'InitWithdrawRequestEvent'
export type InitWithdrawRequestEvent =
  IdlEvents<ValidatorBonds>[typeof INIT_WITHDRAW_REQUEST_EVENT]

export const CANCEL_WITHDRAW_REQUEST_EVENT = 'CancelWithdrawRequestEvent'
export type CancelWithdrawRequestEvent =
  IdlEvents<ValidatorBonds>[typeof CANCEL_WITHDRAW_REQUEST_EVENT]

export const CLAIM_WITHDRAW_REQUEST_EVENT = 'ClaimWithdrawRequestEvent'
export type ClaimWithdrawRequestEvent =
  IdlEvents<ValidatorBonds>[typeof CLAIM_WITHDRAW_REQUEST_EVENT]

export const EMERGENCY_PAUSE_EVENT = 'EmergencyPauseEvent'
export type EmergencyPauseEvent =
  IdlEvents<ValidatorBonds>[typeof EMERGENCY_PAUSE_EVENT]

export const EMERGENCY_RESUME_EVENT = 'EmergencyResumeEvent'
export type EmergencyResumeEvent =
  IdlEvents<ValidatorBonds>[typeof EMERGENCY_RESUME_EVENT]

export const Errors = parseIdlErrors(generated.IDL)

/**
 * Creating Anchor program instance of the Validator Bonds contract.
 * It takes a Provider instance or a Connection and a Wallet.
 * @type {Object} args - Arguments on instruction creation
 * @param param {Connection|Provider} args.connection - connection to solana blockchain that program can be executed on
 *              This can be either Connection instance or Provider instance (when connection is provided, wallet is required)
 * @param param {Wallet|Keypair} args.wallet - wallet to be used as default feePayer and default signers provider
 *               When provider is provided, wallet is not required and it's not used(!) (provider instance is packed with a wallet)
 * @param param {ConfirmOptions} args.opts - connection options for creating transactions for the program
 *               When provider is provided, opts is not required and it's not used(!) (provider instance is packed with connection and opts)
 * @param param {PublicKey} args.programId - program id of the Validator Bonds program
 * @return {ValidatorBondsProgram} - Validator Bonds Anchor program instance
 */
export function getProgram({
  connection,
  wallet,
  opts,
  programId = VALIDATOR_BONDS_PROGRAM_ID,
}: {
  connection: Connection | Provider
  wallet?: AnchorWalletInterface | Keypair
  opts?: ConfirmOptions
  programId?: PublicKey
}): ValidatorBondsProgram {
  let provider: Provider
  if (connection instanceof Connection) {
    if (wallet === undefined) {
      throw new Error(
        'Wallet is required when connection is provided. ' +
          'Please provide a wallet or a provider object.'
      )
    }
    if (wallet instanceof Keypair) {
      wallet = new Wallet(wallet)
    }
    provider = new AnchorProvider(
      connection,
      wallet,
      opts ?? AnchorProvider.defaultOptions()
    )
  } else {
    provider = connection
  }
  return new Program<ValidatorBonds>(generated.IDL, programId, provider)
}

export function bondAddress(
  config: PublicKey,
  voteAccount: PublicKey,
  validatorBondsProgramId: PublicKey = VALIDATOR_BONDS_PROGRAM_ID
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [BOND_SEED, config.toBytes(), voteAccount.toBytes()],
    validatorBondsProgramId
  )
}

export function bondsWithdrawerAuthority(
  config: PublicKey,
  validatorBondsProgramId: PublicKey = VALIDATOR_BONDS_PROGRAM_ID
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [BONDS_WITHDRAWER_AUTHORITY_SEED, config.toBytes()],
    validatorBondsProgramId
  )
}

export function uintToBuffer(number: EpochInfo | number | BN | bigint): Buffer {
  const uintLittleEndian = Buffer.alloc(8)
  const epochBigint =
    typeof number === 'number' ||
    number instanceof BN ||
    typeof number === 'bigint'
      ? BigInt(number.toString())
      : BigInt(number.epoch)
  uintLittleEndian.writeBigUint64LE(epochBigint)
  return uintLittleEndian
}

export function settlementAddress(
  bond: PublicKey,
  merkleRoot: Uint8Array | Buffer | number[],
  epoch: EpochInfo | number | BN | bigint,
  validatorBondsProgramId: PublicKey = VALIDATOR_BONDS_PROGRAM_ID
): [PublicKey, number] {
  if (Array.isArray(merkleRoot)) {
    merkleRoot = new Uint8Array(merkleRoot)
  }
  const epochBuffer = uintToBuffer(epoch)
  return PublicKey.findProgramAddressSync(
    [SETTLEMENT_SEED, bond.toBytes(), merkleRoot, epochBuffer],
    validatorBondsProgramId
  )
}

export function settlementStakerAuthority(
  settlement: PublicKey,
  validatorBondsProgramId: PublicKey = VALIDATOR_BONDS_PROGRAM_ID
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [SETTLEMENT_STAKER_AUTHORITY_SEED, settlement.toBytes()],
    validatorBondsProgramId
  )
}

export function settlementClaimsAddress(
  settlement: PublicKey,
  validatorBondsProgramId: PublicKey = VALIDATOR_BONDS_PROGRAM_ID
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [SETTLEMENT_CLAIMS_SEED, settlement.toBytes()],
    validatorBondsProgramId
  )
}

export function withdrawRequestAddress(
  bond: PublicKey,
  validatorBondsProgramId: PublicKey = VALIDATOR_BONDS_PROGRAM_ID
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [WITHDRAW_REQUEST_SEED, bond.toBytes()],
    validatorBondsProgramId
  )
}

export function bondMintAddress(
  bond: PublicKey,
  validatorIdentity: PublicKey,
  validatorBondsProgramId: PublicKey = VALIDATOR_BONDS_PROGRAM_ID
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [BOND_MINT_SEED, bond.toBytes(), validatorIdentity.toBytes()],
    validatorBondsProgramId
  )
}
