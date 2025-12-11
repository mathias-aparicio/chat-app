local g = import 'github.com/grafana/grafonnet/gen/grafonnet-latest/main.libsonnet';

local dashboard = g.dashboard;
local timeSeries = g.panel.timeSeries;
local query = g.query.prometheus;

local ds = 'prometheus_ds';

dashboard.new('Chat App - Performance Analysis')
+ dashboard.withUid('chat-app-perf')
+ dashboard.withTags(['chat', 'performance'])
+ dashboard.withTimezone('browser')
+ dashboard.withPanels([
  // Panel 1: Throughput
  timeSeries.new('Throughput Rates')
  + timeSeries.queryOptions.withTargets([
    query.new(ds, 'sum(rate(chat_messages_posted_total[30s]))')
    + query.withLegendFormat('Input (HTTP/s)'),
    query.new(ds, 'sum(rate(consumer_messages_processed_total[30s]))')
    + query.withLegendFormat('Output (Processed/s)'),
  ])
  + timeSeries.panelOptions.withGridPos(h=8, w=12, x=0, y=0),

  // Panel 2: Lag
  timeSeries.new('Consumer Lag')
  + timeSeries.queryOptions.withTargets([
    query.new(ds, 'sum(consumer_lag)')
    + query.withLegendFormat('Pending Messages'),
  ])
  + timeSeries.panelOptions.withGridPos(h=8, w=12, x=12, y=0),

  // Panel 3: Time per each tasks
  timeSeries.new('Processing Breakdown (Latency)')
  + timeSeries.queryOptions.withTargets([
    // 1. Total Loop Time
    query.new(ds, 'sum(rate(consumer_processing_duration_seconds_sum[1m])) / sum(rate(consumer_processing_duration_seconds_count[1m]))')
    + query.withLegendFormat('Total Loop Latency'),

    // 2. DB Insert Time
    query.new(ds, 'sum(rate(consumer_db_duration_seconds_sum[1m])) / sum(rate(consumer_db_duration_seconds_count[1m]))')
    + query.withLegendFormat('DB Insert Latency'),

    // 3. Broadcast Time
    query.new(ds, 'sum(rate(consumer_broadcast_duration_seconds_sum[1m])) / sum(rate(consumer_broadcast_duration_seconds_count[1m]))')
    + query.withLegendFormat('Broadcast Latency'),
  ])
  + timeSeries.standardOptions.withUnit('s')
  + timeSeries.panelOptions.withGridPos(h=8, w=24, x=0, y=8),

  // Panel 4: Concurrency Factor
  timeSeries.new('How meany work time per second')
  + timeSeries.queryOptions.withTargets([
    query.new(ds, 'sum(rate(consumer_processing_duration_seconds_sum[1m]))')
    + query.withLegendFormat('Concurrency Level'),
  ])
  + timeSeries.standardOptions.withUnit('none')
  + timeSeries.standardOptions.withMin(0)

  + timeSeries.panelOptions.withGridPos(h=8, w=12, x=0, y=16),
])
