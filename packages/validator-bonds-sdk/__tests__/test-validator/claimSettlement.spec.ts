import { Keypair, LAMPORTS_PER_SOL, PublicKey } from '@solana/web3.js'
import {
  ValidatorBondsProgram,
  CLAIM_SETTLEMENT_EVENT,
  claimSettlementInstruction,
  fundSettlementInstruction,
  findSettlementClaims,
  parseCpiEvents,
  assertEvent,
  settlementClaimsAddress,
  isClaimed,
} from '../../src'
import { initTest } from './testValidator'
import {
  executeInitBondInstruction,
  executeInitConfigInstruction,
  executeInitSettlement,
} from '../utils/testTransactions'
import { executeTxSimple, transaction } from '@marinade.finance/web3js-common'
import { createUserAndFund, signer } from '@marinade.finance/web3js-common'
import {
  MERKLE_ROOT_VOTE_ACCOUNT_1_BUF,
  configAccountKeypair,
  totalClaimVoteAccount1,
  treeNodeBy,
  treeNodesVoteAccount1,
  voteAccount1,
  voteAccount1Keypair,
  withdrawer1,
  withdrawer1Keypair,
  withdrawer2,
  withdrawer2Keypair,
  withdrawer3,
  withdrawer3Keypair,
} from '../utils/merkleTreeTestData'
import {
  createBondsFundedStakeAccount,
  createDelegatedStakeAccount,
  createVoteAccount,
} from '../utils/staking'
import BN from 'bn.js'
import {
  AnchorExtendedProvider,
  getAnchorValidatorInfo,
} from '@marinade.finance/anchor-common'

import assert from 'assert'

// NOTE: order of tests need to be maintained
describe('Validator Bonds claim settlement', () => {
  let provider: AnchorExtendedProvider
  let program: ValidatorBondsProgram
  let configAccount: PublicKey
  let operatorAuthority: Keypair
  let validatorIdentity: Keypair
  let settlementAccount: PublicKey
  let stakeAccount: PublicKey
  let fundSettlementRentPayer: Keypair

  beforeAll(async () => {
    ;({ provider, program } = await initTest())
    ;({ validatorIdentity } = await getAnchorValidatorInfo(provider.connection))
    ;({ configAccount, operatorAuthority } = await executeInitConfigInstruction(
      {
        program,
        provider,
        configAccountKeypair: configAccountKeypair,
        epochsToClaimSettlement: 1,
      }
    ))
    await createVoteAccount({
      voteAccount: voteAccount1Keypair,
      provider,
      validatorIdentity,
    })
    await executeInitBondInstruction({
      configAccount,
      program,
      provider,
      voteAccount: voteAccount1,
      validatorIdentity,
    })
    ;({ settlementAccount } = await executeInitSettlement({
      configAccount,
      program,
      provider,
      voteAccount: voteAccount1,
      operatorAuthority,
      merkleRoot: MERKLE_ROOT_VOTE_ACCOUNT_1_BUF,
      maxMerkleNodes: treeNodesVoteAccount1.length,
      maxTotalClaim: totalClaimVoteAccount1,
    }))
    stakeAccount = await createBondsFundedStakeAccount({
      program,
      provider,
      configAccount: configAccount,
      voteAccount: voteAccount1,
      lamports: totalClaimVoteAccount1.toNumber() + LAMPORTS_PER_SOL * 555,
    })
    const { instruction: fundIx, splitStakeAccount } =
      await fundSettlementInstruction({
        program,
        settlementAccount,
        stakeAccount,
      })
    await provider.sendIx(
      [signer(splitStakeAccount), operatorAuthority],
      fundIx
    )
    // will be used for rent payer of claim settlement
    fundSettlementRentPayer = (await createUserAndFund({
      provider,
      lamports: LAMPORTS_PER_SOL,
    })) as Keypair
  })

  it('claim settlement', async () => {
    await createUserAndFund({
      provider,
      lamports: LAMPORTS_PER_SOL,
      user: withdrawer1Keypair,
    })
    const [
      treeNodeVoteAccount1Withdrawer1,
      treeNodeVoteAccount1Withdrawer1Index,
    ] = treeNodeBy(voteAccount1, withdrawer1)
    const stakeAccountTreeNodeVoteAccount1Withdrawer1 =
      await createDelegatedStakeAccount({
        provider,
        lamports: 4 * LAMPORTS_PER_SOL,
        voteAccount: voteAccount1,
        staker: treeNodeVoteAccount1Withdrawer1.treeNode.stakeAuthority,
        withdrawer: treeNodeVoteAccount1Withdrawer1.treeNode.withdrawAuthority,
      })

    const tx = await transaction(provider)

    const { instruction, settlementClaimsAccount } =
      await claimSettlementInstruction({
        program,
        claimAmount: treeNodeVoteAccount1Withdrawer1.treeNode.data.claim,
        index: treeNodeVoteAccount1Withdrawer1Index,
        merkleProof: treeNodeVoteAccount1Withdrawer1.proof,
        settlementAccount,
        stakeAccountFrom: stakeAccount,
        stakeAccountTo: stakeAccountTreeNodeVoteAccount1Withdrawer1,
      })
    tx.add(instruction)
    const executionReturn = await executeTxSimple(provider.connection, tx, [
      provider.wallet,
      fundSettlementRentPayer,
    ])

    const [settlementClaimAddr] = settlementClaimsAddress(
      settlementClaimsAccount,
      program.programId
    )
    expect(settlementClaimsAccount).toEqual(settlementClaimAddr)

    expect(
      isClaimed(
        program,
        settlementAccount,
        treeNodeVoteAccount1Withdrawer1Index
      )
    ).resolves.toBeTruthy()

    const events = parseCpiEvents(program, executionReturn?.response)
    const e = assertEvent(events, CLAIM_SETTLEMENT_EVENT)
    assert(e !== undefined)
    expect(e.settlement).toEqual(settlementAccount)
    expect(e.amount).toEqual(
      treeNodeVoteAccount1Withdrawer1.treeNode.data.claim
    )
    expect(e.index).toEqual(treeNodeVoteAccount1Withdrawer1Index)
    expect(e.settlement).toEqual(settlementAccount)
    expect(e.settlementLamportsClaimed.old).toEqual(
      new BN(treeNodeVoteAccount1Withdrawer1.treeNode.data.claim).sub(
        treeNodeVoteAccount1Withdrawer1.treeNode.data.claim
      )
    )
    expect(e.settlementLamportsClaimed.new).toEqual(
      treeNodeVoteAccount1Withdrawer1.treeNode.data.claim
    )
    expect(e.settlementMerkleNodesClaimed).toEqual(1)
    expect(e.stakeAccountStaker).toEqual(
      treeNodeVoteAccount1Withdrawer1.treeNode.stakeAuthority
    )
    expect(e.stakeAccountWithdrawer).toEqual(
      treeNodeVoteAccount1Withdrawer1.treeNode.withdrawAuthority
    )
    expect(e.stakeAccountTo).toEqual(
      stakeAccountTreeNodeVoteAccount1Withdrawer1
    )
  })

  it('find claim settlements', async () => {
    await createUserAndFund({
      provider,
      lamports: LAMPORTS_PER_SOL,
      user: withdrawer2Keypair,
    })
    const [treeNodeWithdrawer2, treeNodeWithdrawer2Index] = treeNodeBy(
      voteAccount1,
      withdrawer2
    )
    const stakeAccountTreeNodeWithdrawer2 = await createDelegatedStakeAccount({
      provider,
      lamports: 6 * LAMPORTS_PER_SOL,
      voteAccount: voteAccount1,
      staker: treeNodeWithdrawer2.treeNode.stakeAuthority,
      withdrawer: treeNodeWithdrawer2.treeNode.withdrawAuthority,
    })
    const { instruction: ix1 } = await claimSettlementInstruction({
      program,
      claimAmount: treeNodeWithdrawer2.treeNode.data.claim,
      index: treeNodeWithdrawer2Index,
      merkleProof: treeNodeWithdrawer2.proof,
      settlementAccount,
      stakeAccountFrom: stakeAccount,
      stakeAccountTo: stakeAccountTreeNodeWithdrawer2,
    })
    await createUserAndFund({
      provider,
      lamports: LAMPORTS_PER_SOL,
      user: withdrawer3Keypair,
    })
    const [treeNodeWithdrawer3, treeNodeWithdrawer3Index] = treeNodeBy(
      voteAccount1,
      withdrawer3
    )
    const stakeAccountTreeNodeWithdrawer3 = await createDelegatedStakeAccount({
      provider,
      lamports: 7 * LAMPORTS_PER_SOL,
      voteAccount: voteAccount1,
      staker: treeNodeWithdrawer3.treeNode.stakeAuthority,
      withdrawer: treeNodeWithdrawer3.treeNode.withdrawAuthority,
    })
    const { instruction: ix2 } = await claimSettlementInstruction({
      program,
      claimAmount: treeNodeWithdrawer3.treeNode.data.claim,
      index: treeNodeWithdrawer3Index,
      merkleProof: treeNodeWithdrawer3.proof,
      settlementAccount,
      stakeAccountFrom: stakeAccount,
      stakeAccountTo: stakeAccountTreeNodeWithdrawer3,
    })

    await provider.sendIx([], ix1, ix2)

    const findSettlementList = await findSettlementClaims({
      program,
      settlement: settlementAccount,
    })
    expect(findSettlementList.length).toBeGreaterThanOrEqual(1)
    const findSettlementListAll = await findSettlementClaims({
      program,
    })
    expect(findSettlementListAll.length).toBeGreaterThanOrEqual(1)

    expect(
      isClaimed(program, settlementAccount, treeNodeWithdrawer2Index)
    ).resolves.toBeTruthy()
    expect(
      isClaimed(program, settlementAccount, treeNodeWithdrawer3Index)
    ).resolves.toBeTruthy()
  })
})
