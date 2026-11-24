import type { BarListItem } from '@/components/ui/BarList.vue'
import type { ChartPoint, ChartSeries } from '@/components/ui/TimeSeriesChart.vue'
import { formatNumber, formatPercent, formatShortUtcDay, formatUtcDay, isPast } from '@/lib/format'
import type { Measure } from '@/queries/analytics'
import type { InsightResult } from '@/queries/analytics'

/** The headline number, formatted. */
export interface InsightHeadline {
  label: string
  value: string
}

/** What an insight result shows: a number, then a chart or a ranked list. */
export interface InsightView {
  headline: InsightHeadline
  /** A sentence under the number, such as "of 1,204 known". */
  caption?: string
  chart?: { points: ChartPoint[]; series: ChartSeries[] }
  bars?: { items: BarListItem[]; valueLabel: string }
  /** Set when the range starts before raw retention. */
  missingBefore?: string
}

const MEASURE_LABELS: Record<Measure, string> = {
  total: 'Events',
  users: 'Users',
  share: 'Share of active users',
  per_user: 'Users',
}

const NOT_SET = '(not set)'

function percent(ratio: number): number {
  return Math.round(ratio * 1000) / 10
}

function headline(label: string, value: number, share: boolean): InsightHeadline {
  return { label, value: share ? formatPercent(value) : formatNumber(value) }
}

function pointLabel(start: string, hourly: boolean): string {
  return hourly ? `${start.slice(11, 16)} UTC` : formatShortUtcDay(start)
}

/** Maps a result to what the page draws, by its `view`. */
export function insightView(result: InsightResult): InsightView {
  switch (result.view) {
    case 'series': {
      const share = result.measure === 'share'
      const hourly = result.interval === 'hour'
      return {
        headline: headline(MEASURE_LABELS[result.measure], result.value, share),
        caption: share ? `of ${formatNumber(result.active ?? 0)} active users` : undefined,
        chart: {
          series: [{ name: share ? 'Share (%)' : MEASURE_LABELS[result.measure], tone: 'primary' }],
          // Hours (or days) still ahead would plot as zeros, so the line stops at now.
          points: result.points
            .filter((point) => isPast(point.start))
            .map((point) => ({
              t: point.start,
              label: pointLabel(point.start, hourly),
              values: [share ? percent(point.value) : point.value],
            })),
        },
        missingBefore: result.raw_since ? formatUtcDay(result.raw_since) : undefined,
      }
    }
    case 'breakdown': {
      const share = result.measure === 'share'
      const people = result.measure === undefined || result.measure === null
      const label = people ? 'People' : MEASURE_LABELS[result.measure ?? 'total']
      const rows = [...result.rows, ...(result.other ? [result.other] : [])]
      return {
        headline: headline(label, result.value, share),
        caption: people
          ? `of ${formatNumber(result.active ?? 0)} known`
          : share
            ? `of ${formatNumber(result.active ?? 0)} active users`
            : undefined,
        bars: {
          valueLabel: share ? 'Share (%)' : label,
          items: rows.map((row, at) => {
            const other = row === result.other
            return {
              key: other ? 'other' : `${at}:${row.value ?? ''}`,
              label: other ? 'Other' : (row.value ?? NOT_SET),
              value: share ? percent(row.result) : row.result,
            }
          }),
        },
        missingBefore: result.raw_since ? formatUtcDay(result.raw_since) : undefined,
      }
    }
    case 'distribution':
      return {
        headline: headline('Users', result.users, false),
        caption: 'people with a stored ID who did it at least once',
        bars: {
          valueLabel: 'Users',
          items: result.buckets.map((bucket) => ({
            key: String(bucket.times),
            label: `${bucket.times}${bucket.or_more ? '+' : ''} ${bucket.times === 1 ? 'time' : 'times'}`,
            value: bucket.users,
          })),
        },
        missingBefore: result.raw_since ? formatUtcDay(result.raw_since) : undefined,
      }
    case 'count':
      return {
        headline: headline('People', result.people, false),
        caption: `of ${formatNumber(result.known)} known`,
      }
  }
}
