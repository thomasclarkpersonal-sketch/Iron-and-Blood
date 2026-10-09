// Tests the sdlc-overnight workflow's control flow with mock agents: no
// agent runs, nothing is pushed. Every agent() call returns canned
// structured output, and the checks assert what the script did with it.
//
//   node test/run-tests.mjs [workflow.js] [your .claude/sdlc-overnight.json]
//
// With no arguments it tests this template's workflow against the example
// config and a built-in config that has a CI critic and a regression baseline.
// Pass your project's config to also smoke-test the full run with it.
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
const here = dirname(fileURLToPath(import.meta.url))
const scriptPath = process.argv[2] || join(here, '..', 'workflow', 'sdlc-overnight.js')
const EXAMPLE = readFileSync(join(here, '..', 'config', 'sdlc-overnight.example.json'), 'utf8')
const PROJECT = process.argv[3] ? readFileSync(process.argv[3], 'utf8') : null
const CI_CONFIG = JSON.stringify({
  project: 'a test project with a CI critic',
  runDir: 'out/<slug>-run',
  contract: { files: ['AGENTS.md'], decisions: 'docs/DECISIONS.md', decisionCitation: 'its decision number' },
  docs: { milestones: 'docs/MILESTONE_<n>.md' },
  gate: { env: ['export BUILD_CACHE="$ROOT/.cache/sdlc"'], full: ['make lint', 'make test-all'], docs: ['make docs-check'] },
  engineering: ['Money uses the Money type; never floats.'],
  regression: { name: 'Snapshots', verify: 'make snapshots-verify', rerecord: 'make snapshots-record' },
  critic: { ci: { check: 'Critic', marker: '<!-- example-critic -->', author: 'github-actions', label: 'critic', waiverPrefix: 'critic-waive' } },
  rules: { never: ['editing .github/workflows/critic.yml'], protectedLabels: ['contract-change'] },
})
const src = readFileSync(scriptPath, 'utf8').replace(/^export const meta/m, 'const meta')
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor
const runScript = new AsyncFunction('args', 'agent', 'parallel', 'phase', 'log', 'budget', src)

const T = (id, deps, owns, req, o, m, p, extra = {}) => ({
  id, title: id, summary: id, depends_on: deps, requirements: req, owns,
  contract: { provides: ['x'], consumes: [], acceptance_tests: ['t'] },
  estimate: { optimistic: o, likely: m, pessimistic: p }, blocked_on_decision: '', ...extra,
})
const tasksFor = s => [
  T('m5-1', [], ['docs/ECONOMY_SYSTEM.md'], ['R-F1'], 1, 1, 1),
  T('m5-2', ['m5-1'], ['crates/pax_engine/src/world.rs'], ['R-F1', 'R-N1'], 2, 3, 6),
  T('m5-3', ['m5-1'], ['crates/pax_server/src/api.rs'], ['R-F2'], 1, 2, 3, s.blockM53 ? { blocked_on_decision: 'D33 rate limit scope' } : {}),
  T('m5-4', ['m5-2', 'm5-3'], ['crates/pax_engine/src/systems/tax.rs'], ['R-F2'], 4, 5, 12),
]
// te: 1, 3.333, 2, 6 -> critical m5-1, m5-2, m5-4 = 10.333
const REQS = [{ id: 'R-F1', kind: 'functional', priority: 'must', text: 'a' }, { id: 'R-F2', kind: 'functional', priority: 'must', text: 'b' }, { id: 'R-N1', kind: 'non-functional', priority: 'should', text: 'c' }]
const goodCommit = id => ({ sha: 'abcdef1234', subject: 'engine: add the tax column', why: `Task ${id}: R-F1`, what: 'world.rs', evidence: 'cargo test', docs: 'docs/ECONOMY_SYSTEM.md', decisions: 'none', has_trailer: true, changes_code: true, changes_docs: true })

function scenario(name, opts = {}) {
  const calls = []
  const s = { name, calls, logs: [], clockCalls: 0, ...opts }
  s.agent = async (prompt, o = {}) => {
    const label = o.label || '?'
    calls.push({ label, prompt, opts: o })
    const n = calls.filter(c => c.label === label).length
    const lp = p => label.startsWith(p)
    if (label === 'config') return { text: s.configText ?? CI_CONFIG }
    if (lp('scout:')) return `brief ${label}`
    if (lp('planner:initiation')) return { commit: 'c-init', files: [], summary: 'init', open_questions: [], problem_statement: 'p', urgency: 'medium', objectives: ['o'], scope_in: ['a'], scope_out: ['b'], assumptions: [] }
    if (lp('planner:analysis')) return { commit: 'c-ana', files: [], summary: 'ana', open_questions: [], requirements: REQS, diagrams: ['dfd-context', 'dfd-level-0', 'use-case', 'activity', 'erd'].map(k => ({ kind: k, file: 'f', heading: 'h' })) }
    if (lp('planner:design')) return { commit: 'c-des', files: [], summary: 'des', open_questions: [], proposed_decisions: [], interfaces: [], diagrams: [{ kind: 'class', file: 'f', heading: 'h' }, { kind: 'dialogue', na_reason: 'no UI' }] }
    if (lp('planner:baseline')) return { commit: 'c-bpp', files: [], summary: 'bpp', open_questions: [], tasks: tasksFor(s), critical_path: ['m5-1', 'm5-2', 'm5-4'], expected_duration_hours: 10.33, results_change: false, diagrams: [{ kind: 'gantt', file: 'f', heading: 'h' }, { kind: 'pert', file: 'f', heading: 'h' }] }
    if (lp('reviewer:') || lp('panel:')) return { verdict: 'approve', reviewed_commit: 'x', checks_run: [], findings: [] }
    if (label === 'planner:seal') return { commit: 'c-seal' }
    if (lp('clock')) {
      s.clockCalls++
      return { now: s.stopAfterClocks && s.clockCalls > s.stopAfterClocks ? '2026-10-11T07:05:00+08:00' : '2026-10-10T23:00:00+08:00' }
    }
    if (lp('planner:refresh')) {
      const id = label.split(' ')[1]
      const task = id === 'closeout' ? JSON.parse(/<task>\n([\s\S]*?)\n<\/task>/.exec(prompt)[1]) : tasksFor(s).find(t => t.id === id)
      const q = /<queued-suggestions>\n([\s\S]*?)\n<\/queued-suggestions>/.exec(prompt)[1]
      const adopted = q.startsWith('(none)') ? [] : q.split('\n').map(l => l.replace(/^- \([^)]*\) /, ''))
      return { task, changed_since_plan: 'nothing', adopted_suggestions: adopted, workbook_note: '' }
    }
    if (lp('worker:')) return { status: 'done', commit: 'c', started_at: 't0', finished_at: 't1', summary: 'ok', commits: ['a: b'], docs_updated: ['d'], tests_added: ['t'], checks: [], decisions: label.includes('m5-2') ? ['host keeps the lowest id'] : [], change_requests: [] }
    if (lp('gate')) return { pass: true, commit: 'c', results: [] }
    if (lp('critic')) {
      if (label === 'critic m5-1 local') return { critique_pass: true, findings: [{ tier: 'SUGGESTION', title: 'Name the constant', file: 'f', problem: 'p', failure_scenario: 'f', direction: 'use a const' }], markdown: 'CRITIQUE_PASS' }
      return { critique_pass: true, findings: [], markdown: 'CRITIQUE_PASS' }
    }
    if (lp('audit')) {
      const id = label.split(' ')[1]
      if (s.badAuditFor === id && calls.filter(c => c.label.startsWith(`audit ${id}`)).length === 1) return { commits: [{ ...goodCommit(id), subject: 'x'.repeat(80), docs: '' }] }
      return { commits: [goodCommit(id)] }
    }
    if (lp('reword')) return { commit: 'c', tree_unchanged: true }
    if (lp('publish')) {
      const id = label.split(' ')[1]
      s.prs = (s.prs || 100) + 1
      return { pr: s.prs, url: `https://x/pull/${s.prs}`, head: id }
    }
    if (lp('watch')) {
      const id = label.split(' ')[1]
      if (s.debtFor === id && n === 1) return { head: 'h', checks: 'green', failed_checks: [], critic: 'findings', critic_review: '#### ⚠️ DEBT: Leaky', behind: false }
      if (s.leftDebtFor === id) return { head: 'h', checks: 'green', failed_checks: [], critic: 'findings', critic_review: '#### ⚠️ DEBT: Big refactor', behind: false }
      return { head: 'h', checks: 'green', failed_checks: [], critic: id === 'plan' ? 'skipped' : 'pass', behind: false }
    }
    if (lp('remediation')) {
      if (s.leftDebtFor && label.includes(s.leftDebtFor)) return { commit: 'c', triage: [{ tier: 'DEBT', title: 'Big refactor', action: 'left', reason: 'separate change', waiver: 'Big refactor, separate change' }], gate_pass: true, decisions: [], summary: 's' }
      return { commit: 'c', triage: [{ tier: 'DEBT', title: 'Leaky', action: 'fixed', reason: 'r' }, { tier: 'SUGGESTION', title: 'Split fn', action: 'skipped', reason: 'out of scope' }], gate_pass: true, decisions: [], summary: 's' }
    }
    if (lp('push') || lp('update') || lp('park') || lp('report')) return { done: true, notes: '' }
    if (lp('merge')) return s.refuseMerge ? { merged: false, reason: 'refused: classifier blocked gh pr merge' } : { merged: true, reason: '', merge_commit: 'm' }
    if (label === 'load-plan') return { approved: true, merged: true, commit: 'c', requirements: REQS, tasks: tasksFor(s), critical_path: ['m5-1', 'm5-2', 'm5-4'], expected_duration_hours: 10.33, results_change: false, done: ['m5-1'] }
    throw new Error(`unmocked label ${label}`)
  }
  return s
}

const parallel = async thunks => Promise.all(thunks.map(t => t().catch(() => null)))
const go = (s, args) => runScript(args, s.agent, parallel, () => {}, m => s.logs.push(m), { total: null, spent: () => 0, remaining: () => Infinity })
let failures = 0
const check = (cond, msg) => { if (!cond) { failures++; console.log('  FAIL:', msg) } else console.log('  ok:', msg) }
const labels = s => s.calls.map(c => c.label)
const st = (r, id) => r.tasks[id] && r.tasks[id].status
const BASE_ARGS = { milestone: 'docs/MILESTONE_5.md', stopAt: '2026-10-11T07:00:00+08:00', decisions: 'D27 accepted as proposed' }

{
  const s = scenario('happy: merge authorised, one DEBT round, suggestion carried, audit reword', { debtFor: 'm5-2', badAuditFor: 'm5-3' })
  const r = await go(s, { ...BASE_ARGS, merge: true })
  console.log(s.name); console.log('  status', r.status, JSON.stringify(r.counts))
  if (process.env.DEBUG) { console.log(JSON.stringify(r.tasks, null, 1)); console.log(labels(s).join(' | ')) }
  check(r.slug === 'm5' && r.run_dir === 'out/m5-run', 'slug m5 from milestone, run dir out/m5-run')
  check(r.status === 'complete', 'every task merged, including plan and closeout')
  check(['plan', 'm5-1', 'm5-2', 'm5-3', 'm5-4', 'closeout'].every(id => st(r, id) === 'merged'), 'all six merged')
  check(labels(s).includes('remediation m5-2 #103 r1') && labels(s).includes('push m5-2 r1'), 'DEBT round remediated and pushed')
  check(labels(s).includes('reword m5-3 #1'), 'audit failure triggered a reword')
  const rr = s.calls.find(c => c.label === 'reword m5-3 #1').prompt
  check(rr.includes('the subject must be 1-72 characters') && rr.includes('"Docs:" field is missing'), 'reword told the exact issues')
  const ref2 = s.calls.find(c => c.label.startsWith('planner:refresh m5-2')).prompt
  check(ref2.includes('Name the constant: use a const'), 'local-critic suggestion queued into the next refresh')
  check(s.calls.some(c => c.label.startsWith('planner:refresh m5-') && c.prompt.includes('Split fn (skipped: out of scope)')), 'skipped PR suggestion queued for a later task')
  check(r.decisions_to_check.some(d => d.task === 'm5-2'), 'worker decision recorded for the maintainer')
  const w = s.calls.find(c => c.label === 'worker:m5-2').prompt
  check(w.includes('git switch -c m5/m5-2 origin/main') && w.includes('Why: <the reason') && w.includes('Documentation duty') && w.includes('D27 accepted as proposed'), 'worker gets branch from origin/main, commit rules, docs duty and the maintainer decisions')
  check(w.includes('critic-waive') && w.includes('force-push'), 'worker gets the never-list')
  check(s.calls.find(c => c.label === 'publish m5-2').prompt.includes('## Review rounds'), 'PR body template')
  check(labels(s).filter(l => l.startsWith('report')).length >= 6 && labels(s).includes('report final'), 'report rewritten after every task and at the end')
  check(s.calls.findIndex(c => c.label === 'merge plan #101') < s.calls.findIndex(c => c.label.startsWith('worker:')), 'plan PR merged before any worker')
  check(s.calls.find(c => c.label.startsWith('gate plan')).prompt.includes('make docs-check') && !s.calls.find(c => c.label.startsWith('gate plan')).prompt.includes('make test-all'), 'plan PR gets the docs gate only')
}
{
  const s = scenario('no merge authorisation: plan PR ready, tasks stacked locally')
  const r = await go(s, { ...BASE_ARGS })
  console.log(s.name); console.log('  ', JSON.stringify(r.counts))
  check(st(r, 'plan') === 'ready', 'plan PR left ready for the maintainer')
  check(st(r, 'm5-1') === 'stacked' && st(r, 'm5-2') === 'stacked' && st(r, 'm5-3') === 'stacked', 'chain stacks on the plan branch and m5-1')
  check(s.calls.find(c => c.label === 'worker:m5-2').prompt.includes('git switch -c m5/m5-2 m5/m5-1'), 'm5-2 stacked on m5/m5-1')
  check(st(r, 'm5-4') === 'blocked' && r.tasks['m5-4'].summary.includes('separate branches'), 'm5-4 (two separate stacks) blocked')
  check(!labels(s).some(l => l.startsWith('publish m5')), 'no stacked task published')
  check(!labels(s).some(l => l.startsWith('merge')), 'never merges without authorisation')
  check(!r.tasks.closeout, 'no close-out without merges')
}
{
  const s = scenario('stop time reached', { stopAfterClocks: 3 })
  const r = await go(s, { ...BASE_ARGS, merge: true })
  console.log(s.name); console.log('  ', JSON.stringify(r.counts))
  check(Object.values(r.tasks).some(t => t.status === 'not-started' && t.summary.includes('stop time')), 'later tasks not started')
  check(!r.tasks.closeout || r.tasks.closeout.status === 'not-started', 'no close-out after the stop time')
  check(labels(s).includes('report final'), 'final report still written')
}
{
  const s = scenario('merge refused by classifier', { refuseMerge: true })
  const r = await go(s, { ...BASE_ARGS, merge: true })
  console.log(s.name); console.log('  ', JSON.stringify(r.counts))
  check(st(r, 'plan') === 'ready' && r.tasks.plan.summary.includes('classifier'), 'refusal reported, PR left ready')
  check(st(r, 'm5-1') === 'stacked', 'dependents stack instead of failing')
}
{
  const s = scenario('DEBT left with reason -> needs-waiver', { leftDebtFor: 'm5-2' })
  const r = await go(s, { ...BASE_ARGS, merge: true })
  console.log(s.name); console.log('  ', JSON.stringify(r.counts))
  check(st(r, 'm5-2') === 'needs-waiver', 'needs-waiver')
  check(s.calls.some(c => c.label.startsWith('park m5-2') && c.prompt.includes('"needs-waiver"')), 'parked with needs-waiver label')
  check(r.drafted_waivers.length === 1 && r.drafted_waivers[0].text === 'Big refactor, separate change', 'drafted waiver kept for the report only')
  check(!s.calls.some(c => /^\s*critic-waive/m.test(c.prompt.replace(/"critic-waive[^"]*"/g, ''))), 'no prompt carries a critic-waive line')
  check(st(r, 'm5-4') === 'stacked', 'm5-4 stacks on the waiver-pending branch')
}
{
  const s = scenario('task blocked on a decision', { blockM53: true })
  const r = await go(s, { ...BASE_ARGS, merge: true })
  console.log(s.name); console.log('  ', JSON.stringify(r.counts))
  check(st(r, 'm5-3') === 'blocked-on-decision' && st(r, 'm5-4') === 'blocked', 'blocked task and its dependent')
  check(st(r, 'm5-2') === 'merged', 'independent work continues')
}
{
  const s = scenario('fromPlan with m5-1 already done')
  const r = await go(s, { slug: 'm5', fromPlan: true, merge: true })
  console.log(s.name); console.log('  ', JSON.stringify(r.counts))
  check(!labels(s).some(l => l.startsWith('planner:initiation')) && !labels(s).includes('worker:m5-1'), 'no replanning, done task skipped')
  check(st(r, 'm5-4') === 'merged', 'rest built')
}
{
  const s = scenario('generic project: example config, no CI critic, no regression baseline', { configText: EXAMPLE })
  const r = await go(s, { milestone: 'docs/milestones/M3.md', stopAt: '2026-10-11T07:00:00+08:00', merge: true })
  console.log(s.name); console.log('  status', r.status, JSON.stringify(r.counts))
  check(r.slug === 'm3' && r.run_dir === '.sdlc-runs/m3', 'slug and run dir from the config patterns')
  check(r.status === 'complete', 'complete')
  const w = s.calls.find(c => c.label === 'worker:m5-2').prompt
  check(w.includes('zod schemas') && w.includes('npm test -- --ci') && !w.includes('cargo'), 'worker gets the project\'s engineering rules and gate, nothing from another project')
  check(!s.calls.some(c => c.prompt.includes('critic-waive') || c.prompt.includes('Money type') || c.prompt.includes('make test-all') || c.prompt.includes('DECISIONS.md') || c.prompt.includes('example-critic')), 'no other project\'s specifics leak into any prompt')
  check(!s.calls.find(c => c.label === 'publish m5-2').prompt.includes('--label'), 'no critic label without a CI critic')
  check(s.calls.find(c => c.label.startsWith('watch m5-2')).prompt.includes('always "skipped"'), 'watcher treats the critic as skipped')
  check(s.calls.find(c => c.label.startsWith('gate plan')).prompt.includes('markdown-link-check'), 'plan PR uses the configured docs gate')
  check(s.calls.find(c => c.label === 'worker:m5-2').prompt.includes('ADR number'), 'decision citation style from the config')
  check(!s.calls.find(c => c.label === 'publish m5-2').prompt.includes('## Golden'), 'no regression section without a baseline')
}
{
  const s = scenario('a config with a CI critic and a baseline keeps its specifics')
  const r = await go(s, { ...BASE_ARGS, merge: true })
  const w = s.calls.find(c => c.label === 'worker:m5-2').prompt
  console.log(s.name)
  check(w.includes('Money type') && w.includes('make test-all') && w.includes('critic-waive') && w.includes('BUILD_CACHE') && w.includes('Snapshots must not change'), 'engineering rules, gate, waiver rule, build cache, baseline')
  check(s.calls.find(c => c.label === 'publish m5-2').prompt.includes('--label critic') && s.calls.find(c => c.label.startsWith('watch m5-2')).prompt.includes('<!-- example-critic -->'), 'CI critic label and marker')
  check(w.includes('editing .github/workflows/critic.yml') && w.includes('"contract-change" label'), 'project never-list')
}
if (PROJECT) {
  const s = scenario('your project config: full run', { configText: PROJECT })
  const cfg = JSON.parse(PROJECT)
  const pattern = (cfg.docs && cfg.docs.milestones) || 'docs/MILESTONE_<n>.md'
  const r = await go(s, { milestone: pattern.replace('<n>', '5'), stopAt: '2026-10-11T07:00:00+08:00', merge: true })
  console.log(s.name); console.log('  status', r.status, JSON.stringify(r.counts))
  check(r.status === 'complete', 'the full mock run completes with your config')
  const w = s.calls.find(c => c.label === 'worker:m5-2')
  check(w && cfg.gate.full.every(cmd => w.prompt.includes(cmd)), 'every gate.full command reaches the worker')
}
try { await go(scenario('no config', { configText: '' }), BASE_ARGS); failures++; console.log('  FAIL: ran without a config') } catch (e) { console.log('  ok: rejected a missing config -', e.message.slice(0, 60)) }
try { await go(scenario('bad json', { configText: '{nope' }), BASE_ARGS); failures++; console.log('  FAIL: ran with bad JSON') } catch (e) { console.log('  ok: rejected bad JSON -', e.message.slice(0, 60)) }
for (const bad of [{}, { slug: 'Bad Slug', request: 'x' }, { slug: 'ok' }, { milestone: 'docs/MILESTONE_5.md', stopAt: 'tomorrow 7am' }]) {
  try { await go(scenario('args'), bad); failures++; console.log('  FAIL: accepted', JSON.stringify(bad)) } catch (e) { console.log('  ok: rejected', JSON.stringify(bad), '-', e.message.slice(0, 70)) }
}
console.log(failures ? `${failures} FAILURE(S)` : 'ALL PASSED')
process.exit(failures ? 1 : 0)
