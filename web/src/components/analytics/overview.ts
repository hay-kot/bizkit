import type { StatTile } from '@/components/ui/StatTiles.vue'
import type { ChartPoint, ChartSeries } from '@/components/ui/TimeSeriesChart.vue'
import { openFrom } from '@/lib/chart'
import { formatShortUtcDay } from '@/lib/format'
import type { Summary } from '@/queries/analytics'

/** The Overview chart's metrics: the two a summary has a series for. */
export type OverviewMetric = 'visitors' | 'pageviews'

export const METRICS: Record<OverviewMetric, { label: string; description: string }> = {
  visitors: {
    label: 'Visitors',
    description: 'Distinct visitors in each period. A visitor counts once a day.',
  },
  pageviews: { label: 'Pageviews', description: 'Pages viewed in each period.' },
}

const INTERVAL_MS = { hour: 3_600_000, day: 86_400_000 }

/**
 * The points that have begun at `now`, and the index of the one still
 * running. Buckets still ahead would plot as zeros, so the series stops at now.
 */
function begun(summary: Summary, now: number) {
  const points = summary.series.filter((point) => Date.parse(point.start) < now)
  const partialFrom = openFrom(
    points.map((point) => point.start),
    INTERVAL_MS[summary.interval],
    now,
  )
  return { points, partialFrom }
}

function rounded(value: number): number {
  return Math.round(value * 10) / 10
}

/**
 * The Overview's KPI tiles. `days` is the range's length; the daily
 * average divides by it, so a range ending today counts today as a day.
 * Previous values come only with `compare`.
 */
export function overviewTiles(
  summary: Summary | undefined,
  compare: boolean,
  days: number,
  now: number,
): StatTile[] {
  const totals = summary?.totals
  const previous = compare ? (summary?.previous ?? undefined) : undefined
  const { points, partialFrom } = summary
    ? begun(summary, now)
    : { points: [], partialFrom: undefined }
  const perDay = (visitors: number) => rounded(visitors / Math.max(1, days))
  return [
    {
      label: 'Unique visitors',
      value: totals?.visitors ?? 0,
      previous: previous?.visitors,
      series: points.map((point) => point.visitors),
      partialFrom,
    },
    {
      label: 'Pageviews',
      value: totals?.pageviews ?? 0,
      previous: previous?.pageviews,
      series: points.map((point) => point.pageviews),
      partialFrom,
    },
    {
      label: 'Avg daily visitors',
      value: perDay(totals?.visitors ?? 0),
      previous: previous && perDay(previous.visitors),
    },
    { label: 'Custom events', value: totals?.events ?? 0, previous: previous?.events },
    { label: 'Conversions', value: totals?.conversions ?? 0, previous: previous?.conversions },
  ]
}

/** The Overview chart for one metric, with the previous period when comparing. */
export function overviewChart(
  summary: Summary | undefined,
  metric: OverviewMetric,
  compare: boolean,
  now: number,
): { points: ChartPoint[]; series: ChartSeries[]; partialFrom?: number } {
  const label = METRICS[metric].label
  if (!summary) return { points: [], series: [{ name: label, tone: 'primary' }] }
  const before = compare ? summary.previous_series : undefined
  const { points, partialFrom } = begun(summary, now)
  const hourly = summary.interval === 'hour'
  return {
    series: before
      ? [
          { name: label, tone: 'primary' },
          { name: `Previous ${label.toLowerCase()}`, tone: 'muted' },
        ]
      : [{ name: label, tone: 'primary' }],
    points: points.map((point, i) => ({
      t: point.start,
      label: hourly ? `${point.start.slice(11, 16)} UTC` : formatShortUtcDay(point.start),
      values: before ? [point[metric], before[i]?.[metric] ?? 0] : [point[metric]],
    })),
    partialFrom,
  }
}
