import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { formatShortDate, pad2 } from '../../core/format';
import { isFinished } from '../../core/library.service';
import { Volume } from '../../core/models';

const MAX_ROWS = 4;

/** The stamped reading card: a volume's recent reading sessions. */
@Component({
  selector: 'koma-reading-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="card back-2"></div>
    <div class="card back-1"></div>
    <div class="card front">
      <div class="head mono">
        <b>SESSIONS</b>
        <span>{{ label() }}</span>
      </div>
      <table class="mono">
        <tbody>
          @for (row of rows(); track row.date) {
            <tr>
              <td>{{ row.date }}</td>
              <td>{{ row.pages }}</td>
              <td>{{ row.count }}</td>
            </tr>
          } @empty {
            <tr>
              <td>NO SESSIONS YET</td>
            </tr>
          }
        </tbody>
      </table>
      <div class="stamp">{{ stamp() }}</div>
    </div>
  `,
  styles: `
    :host {
      position: relative;
      display: block;
      height: 190px;
      margin: 0 6px;
    }
    .card {
      position: absolute;
      inset: 0;
      border: var(--border-control) solid var(--ink);
      background: var(--panel2);
      transition: transform var(--t-slam) var(--ease);
    }
    .back-2 {
      background: var(--acc);
      transform: rotate(6deg) translate(16px, 6px);
    }
    .back-1 {
      background: var(--bg2);
      transform: rotate(-4deg) translate(-10px, 4px);
    }
    /* The stack fans out on hover. */
    :host(:hover) .back-2 {
      transform: rotate(12deg) translate(70px, 10px);
    }
    :host(:hover) .back-1 {
      transform: rotate(-10deg) translate(-60px, 6px);
    }
    .front {
      padding: 12px;
    }
    .head {
      display: flex;
      justify-content: space-between;
      padding-bottom: 8px;
      border-bottom: var(--border-control) solid var(--ink);
      font-size: 11px;
    }
    table {
      width: 100%;
      margin-top: 6px;
      border-collapse: collapse;
      font-size: 11px;
    }
    td {
      padding: 5px 2px;
      border-bottom: 1px solid var(--line);
    }
    .stamp {
      position: absolute;
      right: 12px;
      bottom: 12px;
      padding: 6px 12px;
      border: 3px solid var(--acc);
      background: color-mix(in srgb, var(--panel2) 85%, transparent);
      color: var(--acc);
      font-family: var(--font-display);
      font-size: 20px;
      text-transform: uppercase;
      transform: rotate(-12deg);
      animation: stamp-in var(--t-slam) var(--ease-slam) 300ms both;
    }
    @keyframes stamp-in {
      from {
        opacity: 0;
        transform: rotate(-12deg) scale(1.8);
      }
    }
  `,
})
export class ReadingCard {
  readonly volume = input.required<Volume>();

  protected readonly label = computed(() => {
    const volume = this.volume();
    return `${volume.unit === 'chapter' ? 'CH' : 'VOL'} ${pad2(volume.number)}`;
  });

  protected readonly rows = computed(() =>
    [...this.volume().sessions]
      .reverse()
      .slice(0, MAX_ROWS)
      .map((session) => ({
        date: formatShortDate(session.date),
        pages: `p.${session.from}–${session.to}`,
        count: `${Math.max(0, session.to - session.from)} pp`,
      })),
  );

  protected readonly stamp = computed(() => {
    const volume = this.volume();
    if (isFinished(volume)) return 'Finished';
    return volume.progress ? 'In progress' : 'Unread';
  });
}
