import type { BarListItem } from '@/components/ui/BarList.vue'
import type { ChartPoint, ChartSeries } from '@/components/ui/TimeSeriesChart.vue'
import { formatNumber, formatPercent, formatShortUtcDay, formatUtcDay } from '@/lib/format'
import type { InsightDisplay, InsightResult, Measure } from '@/queries/analytics'

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
  chart?: { points: ChartPoint[]; series: ChartSeries[]; partialFrom?: number }
  bars?: { items: BarListItem[]; valueLabel: string }
  /** Set when the range starts before raw retention. */
  missingBefore?: string
  /**
   * The number a trend compares with the previous period; none for a
   * count, which ignores the range.
   */
  trendValue?: number
  /** Shares change in percentage points, everything else relative. */
  trendMode: 'relative' | 'points'
}

/** How a result draws. `list` is the ranked `BarList`; the API stores it as `bar`. */
export type Display = 'line' | 'bar' | 'number' | 'list'

const OPTIONS: Record<InsightResult['view'], Display[]> = {
  series: ['line', 'bar', 'number'],
  breakdown: ['list', 'number'],
  distribution: ['list', 'number'],
  count: ['number'],
}

/** The displays a view can draw, its default first. */
export function displayOptions(view: InsightResult['view']): Display[] {
  return OPTIONS[view]
}

/** The display a result draws with: the saved one when the view can draw it, else the view's default. */
export function resolveDisplay(saved: InsightDisplay, view: InsightResult['view']): Display {
  const options = OPTIONS[view]
  const wanted: Display | undefined =
    saved === 'auto' ? undefined : saved === 'bar' && view !== 'series' ? 'list' : saved
  return wanted && options.includes(wanted) ? wanted : (options[0] ?? 'number')
}

/** What saving a picked display stores. */
export function storedDisplay(display: Display): InsightDisplay {
  return display === 'list' ? 'bar' : display
}

const INTERVAL_MS = { hour: 3_600_000, day: 86_400_000 } as const

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

/**
 * Maps a result to what the page draws, by its `view`. `now` (ms) cuts
 * off buckets still ahead and marks the one still open.
 */
export function insightView(result: InsightResult, now: number): InsightView {
  switch (result.view) {
    case 'series': {
      const share = result.measure === 'share'
      const hourly = result.interval === 'hour'
      // Hours (or days) still ahead would plot as zeros, so the line stops at now.
      const kept = result.points.filter((point) => Date.parse(point.start) < now)
      const last = kept[kept.length - 1]
      const open = last && Date.parse(last.start) + INTERVAL_MS[result.interval] > now
      return {
        headline: headline(MEASURE_LABELS[result.measure], result.value, share),
        caption: share ? `of ${formatNumber(result.active ?? 0)} active users` : undefined,
        chart: {
          series: [
            share
              ? { name: 'Share', tone: 'primary', unit: '%' }
              : { name: MEASURE_LABELS[result.measure], tone: 'primary' },
          ],
          points: kept.map((point) => ({
            t: point.start,
            label: pointLabel(point.start, hourly),
            values: [share ? percent(point.value) : point.value],
          })),
          partialFrom: open ? kept.length - 1 : undefined,
        },
        missingBefore: result.raw_since ? formatUtcDay(result.raw_since) : undefined,
        trendValue: result.value,
        trendMode: share ? 'points' : 'relative',
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
        trendValue: result.value,
        trendMode: share ? 'points' : 'relative',
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
        trendValue: result.users,
        trendMode: 'relative',
      }
    case 'count':
      return {
        headline: headline('People', result.people, false),
        caption: `of ${formatNumber(result.known)} known`,
        trendMode: 'relative',
      }
  }
}
