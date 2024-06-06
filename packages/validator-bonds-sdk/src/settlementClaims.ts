import BN from 'bn.js'
import { SettlementClaims, ValidatorBondsProgram } from './sdk'
import { AccountInfo } from '@solana/web3.js'

// discriminator + metadata
export const HEADER_DATA_SIZE = 56

export type SettlementClaimsBitmap = {
  account: SettlementClaims
  bitmap: Bitmap
}

export class Bitmap {
  bitmapData: Buffer
  maxRecords: number

  /**
   * Get data for bitmap that is restricted to the size of the bitmap defined by SettlementClaims account.
   * All other methods within the Bitmap class consider data is already restricted.
   */
  public constructor(settlementClaims: SettlementClaims, data: Buffer) {
    const { div, mod } = settlementClaims.maxRecords.divmod(new BN(8))
    const bytes = div.toNumber() + (mod.toNumber() > 0 ? 1 : 0)
    this.bitmapData = data.subarray(HEADER_DATA_SIZE, HEADER_DATA_SIZE + bytes)
    this.maxRecords = settlementClaims.maxRecords.toNumber()
  }

  get(index: number): boolean {
    this.assertValidIndex(index)
    const byteIndex = Math.floor(index / 8)
    const bitIndex = index % 8
    console.log(
      'byteIndex',
      byteIndex,
      'bitIndex',
      bitIndex,
      'byte data',
      this.bitmapAsBits()
    )
    return (this.bitmapData[byteIndex] & (1 << bitIndex)) !== 0
  }

  bitmapAsBits(): string[] {
    const result: string[] = []
    const data = this.bitmapData
    console.log('bitmap data', this.bitmapData)
    for (let i = 0; i < data.length; i++) {
      result.push(byte2bits(data[i]))
    }
    return result
  }

  assertValidIndex(index: number) {
    if (index < 0 || index >= this.maxRecords) {
      throw new Error(`Index ${index} out of range`)
    }
  }
}

export function decodeSettlementClaimsData(
  program: ValidatorBondsProgram,
  accountInfo: AccountInfo<Buffer>
): SettlementClaimsBitmap {
  const account = program.coder.accounts.decode<SettlementClaims>(
    'settlementClaims',
    accountInfo.data
  )

  return {
    account,
    bitmap: new Bitmap(account, accountInfo.data),
  }
}

function byte2bits(a: number) {
  let tmp = ''
  for (let i = 128; i >= 1; i /= 2) tmp += a & i ? '1' : '0'
  return tmp
}
