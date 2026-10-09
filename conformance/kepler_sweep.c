// Kepler stepped orbit: per-period energy-error statistics for the sweep in kepler_sweep.py.
//
// usage: kepler_sweep <kdk|dkd|rk4> <e> <periods> <n>...
// For each evaluation order o of 1/r^3 (0: 1/(r2*sqrt(r2)), 1: 1/pow(r2, 1.5),
// 2: 1/(r*r*r) with r = sqrt(r2), 3: 1/sqrt(r2*r2*r2)) and each n, prints one line:
//   method o e n max2n mean2n max_1 mean_1 max_2 mean_2 ... max_P mean_P
// max_p / mean_p: largest / mean relative energy error |(E + 1/2) / (1/2)| over the steps
// of period p of a run with n steps per period; max2n / mean2n: the same for period 1 of a
// run with 2n steps per period. mu = a = 1, start at apoapsis, h = 2 pi / n.
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int ORD, M;

static inline double inv_r3(double r2) {
  switch (ORD) {
    case 0: return 1.0 / (r2 * sqrt(r2));
    case 1: return 1.0 / pow(r2, 1.5);
    case 2: { double r = sqrt(r2); return 1.0 / (r * r * r); }
    default: return 1.0 / sqrt(r2 * r2 * r2);
  }
}
static inline void acc(double x, double y, double *ax, double *ay) {
  double k = inv_r3(x * x + y * y); *ax = -x * k; *ay = -y * k;
}
static inline double rel(const double *s) {
  double r = sqrt(s[0] * s[0] + s[1] * s[1]);
  double E = (s[2] * s[2] + s[3] * s[3]) / 2 - 1 / r;
  return fabs((E + 0.5) / 0.5);
}
static void step(double *s, double h, double *a) {
  if (M == 0) {         /* kick-drift-kick */
    s[2] += h / 2 * a[0]; s[3] += h / 2 * a[1]; s[0] += h * s[2]; s[1] += h * s[3];
    acc(s[0], s[1], &a[0], &a[1]); s[2] += h / 2 * a[0]; s[3] += h / 2 * a[1];
  } else if (M == 1) {  /* drift-kick-drift */
    double ax, ay;
    s[0] += h / 2 * s[2]; s[1] += h / 2 * s[3]; acc(s[0], s[1], &ax, &ay);
    s[2] += h * ax; s[3] += h * ay; s[0] += h / 2 * s[2]; s[1] += h / 2 * s[3];
  } else {              /* classical RK4 */
    const double c[4] = {0, 0.5, 0.5, 1};
    double k[4][4], t[4];
    for (int j = 0; j < 4; j++) {
      for (int i = 0; i < 4; i++) t[i] = s[i] + (j ? c[j] * h * k[j - 1][i] : 0);
      k[j][0] = t[2]; k[j][1] = t[3]; acc(t[0], t[1], &k[j][2], &k[j][3]);
    }
    for (int i = 0; i < 4; i++) s[i] += h / 6 * (k[0][i] + 2 * k[1][i] + 2 * k[2][i] + k[3][i]);
  }
}
static void run(double e, long n, int P, double *pm, double *pa) {
  const double pi = 3.141592653589793;
  double h = 2 * pi / n, r0 = 1 + e, a[2];
  double s[4] = {r0, 0, 0, sqrt(2 / r0 - 1)};
  acc(s[0], s[1], &a[0], &a[1]);
  for (int p = 0; p < P; p++) {
    double m = 0, sm = 0;
    for (long i = 0; i < n; i++) { step(s, h, a); double r = rel(s); sm += r; if (r > m) m = r; }
    pm[p] = m; pa[p] = sm / n;
  }
}
int main(int argc, char **argv) {
  if (argc < 5) { fprintf(stderr, "usage: %s <kdk|dkd|rk4> <e> <periods> <n>...\n", argv[0]); return 2; }
  M = !strcmp(argv[1], "kdk") ? 0 : !strcmp(argv[1], "dkd") ? 1 : 2;
  double e = atof(argv[2]);
  int P = atoi(argv[3]);
  if (P < 1 || P > 64) { fprintf(stderr, "periods must be 1..64\n"); return 2; }
  double pm[64], pa[64], f2[1], g2[1];
  for (int o = 0; o < 4; o++) {
    ORD = o;
    for (int k = 4; k < argc; k++) {
      long n = atol(argv[k]);
      run(e, 2 * n, 1, f2, g2);
      run(e, n, P, pm, pa);
      printf("%s %d %.3f %ld %.10g %.10g", argv[1], o, e, n, f2[0], g2[0]);
      for (int p = 0; p < P; p++) printf(" %.10g %.10g", pm[p], pa[p]);
      printf("\n");
    }
  }
  return 0;
}
