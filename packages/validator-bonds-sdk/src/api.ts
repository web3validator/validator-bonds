import { ProgramAccount } from '@coral-xyz/anchor'
import { EpochInfo, PublicKey } from '@solana/web3.js'
import {
  ValidatorBondsProgram,
  Config,
  Bond,
  WithdrawRequest,
  bondAddress,
  withdrawRequestAddress,
  settlementAddress,
  Settlement,
  SettlementClaim,
} from './sdk'
import BN from 'bn.js'
import { bs58 } from '@coral-xyz/anchor/dist/cjs/utils/bytes'

// TODO:
//   - users can create arbitrary stake accounts (even with lockups), sdk must be prepared for that when showing total usable deposits

export async function getConfig(
  program: ValidatorBondsProgram,
  address: PublicKey
): Promise<Config> {
  return program.account.config.fetch(address)
}

export async function findConfigs({
  program,
  adminAuthority,
  operatorAuthority,
}: {
  program: ValidatorBondsProgram
  adminAuthority?: PublicKey
  operatorAuthority?: PublicKey
}): Promise<ProgramAccount<Config>[]> {
  const filters = []
  if (adminAuthority) {
    filters.push({
      memcmp: {
        bytes: adminAuthority.toBase58(),
        // 8 anchor offset
        offset: 8,
      },
    })
  }
  if (operatorAuthority) {
    filters.push({
      memcmp: {
        bytes: operatorAuthority.toBase58(),
        // 8 anchor offset + first data 32B admin pubkey
        offset: 40,
      },
    })
  }
  return await program.account.config.all(filters)
}

export async function getBond(
  program: ValidatorBondsProgram,
  address: PublicKey
): Promise<Bond> {
  return program.account.bond.fetch(address)
}

export async function findBonds({
  program,
  config,
  voteAccount,
  bondAuthority,
}: {
  program: ValidatorBondsProgram
  config?: PublicKey
  voteAccount?: PublicKey
  bondAuthority?: PublicKey
}): Promise<ProgramAccount<Bond>[]> {
  if (config && voteAccount) {
    const [bondAccount] = bondAddress(config, voteAccount, program.programId)
    const bondData = await getBond(program, bondAccount)
    return [{ publicKey: bondAccount, account: bondData }]
  }
  const filters = []
  if (config) {
    filters.push({
      memcmp: {
        bytes: config.toBase58(),
        // 8 anchor offset
        offset: 8,
      },
    })
  }
  if (voteAccount) {
    filters.push({
      memcmp: {
        bytes: voteAccount.toBase58(),
        // 8 anchor offset + first data 32B config pubkey
        offset: 40,
      },
    })
  }
  if (bondAuthority) {
    filters.push({
      memcmp: {
        bytes: bondAuthority.toBase58(),
        // 8 anchor offset + 32B config pubkey + 32B validator vote pubkey
        offset: 72,
      },
    })
  }
  return await program.account.bond.all(filters)
}

export async function getWithdrawRequest(
  program: ValidatorBondsProgram,
  address: PublicKey
): Promise<WithdrawRequest> {
  return program.account.withdrawRequest.fetch(address)
}

export async function findWithdrawRequests({
  program,
  voteAccount,
  bond,
  epoch,
}: {
  program: ValidatorBondsProgram
  voteAccount?: PublicKey
  bond?: PublicKey
  epoch?: number | BN
}): Promise<ProgramAccount<WithdrawRequest>[]> {
  if (bond) {
    const [withdrawRequestAccount] = withdrawRequestAddress(
      bond,
      program.programId
    )
    const withdrawRequestData = await getWithdrawRequest(
      program,
      withdrawRequestAccount
    )
    return [{ publicKey: withdrawRequestAccount, account: withdrawRequestData }]
  }
  const filters = []
  if (voteAccount) {
    filters.push({
      memcmp: {
        bytes: voteAccount.toBase58(),
        // 8 anchor offset
        offset: 8,
      },
    })
  }
  if (epoch) {
    filters.push({
      memcmp: {
        bytes: bs58.encode(new BN(epoch).toArray('le', 8)), // TODO: consider to use the same number-to-bytes as in settlement address
        // 8 anchor offset + 32B validator vote pubkey + 32B bond pubkey
        offset: 72,
      },
    })
  }
  return await program.account.withdrawRequest.all(filters)
}

export async function getSettlement(
  program: ValidatorBondsProgram,
  address: PublicKey
): Promise<Settlement> {
  return program.account.settlement.fetch(address)
}

export async function findSettlements({
  program,
  bond,
  merkleRoot,
  epoch,
}: {
  program: ValidatorBondsProgram
  bond?: PublicKey
  merkleRoot?: Uint8Array | Buffer | number[]
  epoch?: number | BN | EpochInfo
}): Promise<ProgramAccount<Settlement>[]> {
  if (bond && merkleRoot && epoch) {
    const [settlementAccount] = settlementAddress(
      bond,
      merkleRoot,
      epoch,
      program.programId
    )
    const settlementData = await getSettlement(program, settlementAccount)
    return [{ publicKey: settlementAccount, account: settlementData }]
  }
  const filters = []
  if (bond) {
    filters.push({
      memcmp: {
        bytes: bond.toBase58(),
        // 8 anchor offset
        offset: 8,
      },
    })
  }
  if (merkleRoot) {
    filters.push({
      memcmp: {
        bytes: bs58.encode(merkleRoot),
        // 8 anchor offset + 32B bond pubkey + 32B settlement authority
        offset: 72,
      },
    })
  }
  return await program.account.settlement.all(filters)
}

export async function getSettlementClaim(
  program: ValidatorBondsProgram,
  address: PublicKey
): Promise<SettlementClaim> {
  return program.account.settlementClaim.fetch(address)
}

export async function findSettlementClaims({
  program,
  settlement,
  stakeAuthority,
  withdrawAuthority,
  voteAccount,
}: {
  program: ValidatorBondsProgram
  settlement?: PublicKey
  stakeAuthority?: PublicKey
  withdrawAuthority?: PublicKey
  voteAccount?: PublicKey
}): Promise<ProgramAccount<SettlementClaim>[]> {
  const filters = []
  if (settlement) {
    filters.push({
      memcmp: {
        bytes: settlement.toBase58(),
        // 8 anchor offset
        offset: 8,
      },
    })
  }
  if (stakeAuthority) {
    filters.push({
      memcmp: {
        bytes: stakeAuthority.toBase58(),
        // 8 anchor offset + settlement 32B
        offset: 40,
      },
    })
  }
  if (withdrawAuthority) {
    filters.push({
      memcmp: {
        bytes: withdrawAuthority.toBase58(),
        // 8 anchor offset + 32B settlement + 32B stake authority
        offset: 72,
      },
    })
  }
  if (voteAccount) {
    filters.push({
      memcmp: {
        bytes: voteAccount.toBase58(),
        // 8 anchor offset + 32B settlement + 32B stake authority + withdraw authority 32B
        offset: 104,
      },
    })
  }
  return await program.account.settlementClaim.all(filters)
}
