import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { formatSize } from '../../core/format';
import { seriesStats } from '../../core/library.service';
import { Series } from '../../core/models';

const STATUS_LABEL = { reading: 'OUT', finished: 'FINISHED', unread: 'ON SHELF' } as const;

/** The catalog table: one row per series, like a library ledger. */
@Component({
  selector: 'koma-series-table',
  imports: [RouterLink],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <table>
      <thead>
        <tr>
          <th>ID</th>
          <th>SERIES</th>
          <th>TYPE</th>
          <th>VOLS</th>
          <th>READ</th>
          <th>SIZE</th>
          <th>STATUS</th>
        </tr>
      </thead>
      <tbody>
        @for (row of rows(); track row.id) {
          <tr (click)="open(row.id)">
            <td>{{ row.number }}</td>
            <td>
              <a [routerLink]="['/series', row.id]" (click)="$event.stopPropagation()">{{ row.title }}</a>
            </td>
            <td>{{ row.type }}</td>
            <td>{{ row.volumes }}</td>
            <td>{{ row.finished }}/{{ row.volumes }}</td>
            <td>{{ row.size }}</td>
            <td>
              <span class="status" [class.out]="row.status === 'OUT'">{{ row.status }}</span>
            </td>
          </tr>
        }
      </tbody>
    </table>
  `,
  styles: `
    table {
      width: 100%;
      border-collapse: collapse;
      font-family: var(--font-mono);
      font-size: 12px;
    }
    th {
      padding: 9px 8px;
      background: var(--ink);
      color: var(--on-ink);
      font-weight: 500;
      text-align: left;
    }
    td {
      padding: 10px 8px;
      border-bottom: 1px solid color-mix(in srgb, var(--ink) 40%, transparent);
    }
    tbody tr {
      cursor: pointer;
    }
    tbody tr:hover td {
      background: var(--acc);
      color: var(--on-acc);
    }
    a {
      color: inherit;
      font-weight: 700;
      text-decoration: none;
      text-transform: uppercase;
    }
    .status {
      display: inline-block;
      padding: 2px 6px;
      border: 1.5px solid currentColor;
      font-size: 11px;
    }
    .status.out {
      border-color: var(--acc);
      background: var(--acc);
      color: var(--on-acc);
    }
    tr:hover .status.out {
      border-color: var(--on-acc);
    }
  `,
})
export class SeriesTable {
  private readonly router = inject(Router);

  readonly series = input.required<Series[]>();

  protected readonly rows = computed(() =>
    this.series().map((s, i) => {
      const stats = seriesStats(s);
      return {
        id: s.id,
        number: String(i + 1).padStart(3, '0'),
        title: s.title,
        type: s.type.toUpperCase(),
        volumes: stats.volumes,
        finished: stats.finished,
        size: formatSize(stats.size),
        status: STATUS_LABEL[stats.status],
      };
    }),
  );

  protected open(seriesId: string): void {
    void this.router.navigate(['/series', seriesId]);
  }
}
