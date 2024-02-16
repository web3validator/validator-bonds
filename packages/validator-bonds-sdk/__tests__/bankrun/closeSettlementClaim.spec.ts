import {
  Config,
  Errors,
  ValidatorBondsProgram,
  claimSettlementInstruction,
  closeSettlementClaimInstruction,
  closeSettlementInstruction,
  fundSettlementInstruction,
  getConfig,
} from '../../src'
import {
  BankrunExtendedProvider,
  assertNotExist,
  currentEpoch,
  initBankrunTest,
  warpOffsetEpoch,
  warpToNextEpoch,
} from './bankrun'
import {
  createUserAndFund,
  executeInitBondInstruction,
  executeInitConfigInstruction,
  executeInitSettlement,
} from '../utils/testTransactions'
import { ProgramAccount } from '@coral-xyz/anchor'
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from '@solana/web3.js'
import {
  createBondsFundedStakeAccount,
  createStakeAccount,
  createVoteAccount,
} from '../utils/staking'
import { signer } from '@marinade.finance/web3js-common'
import {
  MERKLE_ROOT_BUF,
  configAccountKeypair,
  totalClaimVoteAccount1,
  treeNodeBy,
  treeNodesVoteAccount1,
  voteAccount1Keypair,
  withdrawer1,
  withdrawer1Keypair,
  withdrawer2Keypair,
  withdrawer3Keypair,
} from '../utils/merkleTreeTestData'
import { verifyError } from '@marinade.finance/anchor-common'

describe('Validator Bonds close settlement claim', () => {
  const epochsToClaimSettlement = 3
  let provider: BankrunExtendedProvider
  let program: ValidatorBondsProgram
  let config: ProgramAccount<Config>
  let operatorAuthority: Keypair
  let validatorIdentity1: Keypair
  let voteAccount1: PublicKey
  let settlementAccount1: PublicKey
  let settlementEpoch: number
  let stakeAccount1: PublicKey

  beforeAll(async () => {
    ;({ provider, program } = await initBankrunTest())
    await warpToNextEpoch(provider)
    const { configAccount, operatorAuthority: operatorAuth } =
      await executeInitConfigInstruction({
        program,
        provider,
        epochsToClaimSettlement,
        configAccountKeypair: configAccountKeypair,
      })
    operatorAuthority = operatorAuth
    config = {
      publicKey: configAccount,
      account: await getConfig(program, configAccount),
    }
    ;({ voteAccount: voteAccount1, validatorIdentity: validatorIdentity1 } =
      await createVoteAccount({
        voteAccount: voteAccount1Keypair,
        provider,
      }))
    await executeInitBondInstruction({
      program,
      provider,
      config: config.publicKey,
      voteAccount: voteAccount1,
      validatorIdentity: validatorIdentity1,
    })

    settlementEpoch = await currentEpoch(provider)
    ;({ settlementAccount: settlementAccount1 } = await executeInitSettlement({
      config: config.publicKey,
      program,
      provider,
      voteAccount: voteAccount1,
      operatorAuthority,
      currentEpoch: settlementEpoch,
      merkleRoot: MERKLE_ROOT_BUF,
      maxMerkleNodes: treeNodesVoteAccount1.length,
      maxTotalClaim: totalClaimVoteAccount1,
    }))
    stakeAccount1 = await createBondsFundedStakeAccount({
      program,
      provider,
      configAccount: config.publicKey,
      voteAccount: voteAccount1,
      lamports: totalClaimVoteAccount1.toNumber() + LAMPORTS_PER_SOL * 5,
    })

    await warpToNextEpoch(provider) // activate stake account

    const { instruction: fundIx1, splitStakeAccount: split1 } =
      await fundSettlementInstruction({
        program,
        settlementAccount: settlementAccount1,
        stakeAccount: stakeAccount1,
      })
    await provider.sendIx([signer(split1), operatorAuthority], fundIx1)
    if ((await provider.context.banksClient.getAccount(withdrawer1)) === null) {
      await createUserAndFund(provider, LAMPORTS_PER_SOL, withdrawer1Keypair)
      await createUserAndFund(provider, LAMPORTS_PER_SOL, withdrawer2Keypair)
      await createUserAndFund(provider, LAMPORTS_PER_SOL, withdrawer3Keypair)
    }
  })

  it('close settlement claim', async () => {
    const treeNode1Withdrawer1 = treeNodeBy(voteAccount1, withdrawer1)
    const stakeAccountTreeNode1Withdrawer1 = await createStakeAccount({
      provider,
      lamports: 123 * LAMPORTS_PER_SOL,
      voteAccount: voteAccount1,
      newStakerAuthority: treeNode1Withdrawer1.treeNode.stakeAuthority,
      newWithdrawerAuthority: treeNode1Withdrawer1.treeNode.withdrawAuthority,
    })
    await warpToNextEpoch(provider)
    const { instruction: claimIx, settlementClaimAccount } =
      await claimSettlementInstruction({
        program,
        claimAmount: treeNode1Withdrawer1.treeNode.data.claim,
        merkleProof: treeNode1Withdrawer1.proof,
        settlementAccount: settlementAccount1,
        stakeAccountFrom: stakeAccount1,
        stakeAccountTo: stakeAccountTreeNode1Withdrawer1,
        stakeAccountStaker: treeNode1Withdrawer1.treeNode.stakeAuthority,
        stakeAccountWithdrawer: treeNode1Withdrawer1.treeNode.withdrawAuthority,
      })
    await provider.sendIx([], claimIx)
    expect(
      provider.connection.getAccountInfo(settlementClaimAccount)
    ).resolves.not.toBeNull()

    const { instruction: closeIx } = await closeSettlementClaimInstruction({
      program,
      settlementAccount: settlementAccount1,
      settlementClaimAccount,
    })

    try {
      await provider.sendIx([], closeIx)
      throw new Error('Failure expected; the settlement has not been closed')
    } catch (e) {
      verifyError(e, Errors, 6027, 'settlement to be closed')
    }

    await warpToNotBeClaimable() // we can close settlement here
    const { instruction: closeSettle1 } = await closeSettlementInstruction({
      program,
      settlementAccount: settlementAccount1,
      splitRentRefundAccount: stakeAccount1,
    })
    await provider.sendIx([], closeSettle1, closeIx)
    await assertNotExist(provider, settlementAccount1)
    await assertNotExist(provider, settlementClaimAccount)
  })

  async function warpToNotBeClaimable() {
    await warpOffsetEpoch(provider, epochsToClaimSettlement + 1)
  }
})
