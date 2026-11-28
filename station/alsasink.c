/*
 * alsasink — постоянный 32-битный armhf ALSA-помощник для jarvisd.
 *
 * Загружает штатную /usr/lib/libasound.so.2 через dlopen и избавляет jarvisd от fork+exec
 * aplay на каждую фразу и amixer на каждую проверку громкости.
 *
 * Протокол (бинарный, по stdin/stdout, без буферизации):
 *   Заголовок команды: 1 байт кода + 4 байта uint32 LE (arg)
 *     'O' (0x4F) + rate:      открыть PCM default (mono S16_LE, rate Гц, 100 мс буфер) → "OK\n" / "ERR ...\n"
 *     'W' (0x57) + byte_len:  записать byte_len байт PCM (без ответа — конвейерный поток, XRUN чинится сам)
 *     'X' (0x58) + 0:         мгновенный стоп (snd_pcm_drop + close) → "OK\n"
 *     'D' (0x44) + 0:         доиграть буфер и закрыть (snd_pcm_drain + close) → "OK\n"
 *     'V' (0x56) + 0:         прочитать Master в % (0..100, или -1) → "VOL <n>\n"
 *     'Q' (0x51) + 0:         выход
 *
 *   Также поддерживает однократный запуск `alsasink --vol` → печатает процент Master и выходит.
 *
 * Сборка (как и spotcmd, 32-битный ARM под glibc 2.30 колонки):
 *   zig cc -target arm-linux-gnueabihf.2.30 -mcpu=cortex_a7 -O2 -s -o alsasink station/alsasink.c -ldl
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

typedef void *snd_pcm_t;
typedef void *snd_mixer_t;
typedef void *snd_mixer_elem_t;
typedef void *snd_mixer_selem_id_t;
typedef unsigned long snd_pcm_uframes_t;
typedef long snd_pcm_sframes_t;

#define SND_PCM_STREAM_PLAYBACK 0
#define SND_PCM_ACCESS_RW_INTERLEAVED 3
#define SND_PCM_FORMAT_S16_LE 2
#define SND_MIXER_SCHN_FRONT_LEFT 0

static int (*fn_pcm_open)(snd_pcm_t **, const char *, int, int);
static int (*fn_pcm_close)(snd_pcm_t *);
static int (*fn_pcm_set_params)(snd_pcm_t *, int, int, unsigned int, unsigned int, int, unsigned int);
static snd_pcm_sframes_t (*fn_pcm_writei)(snd_pcm_t *, const void *, snd_pcm_uframes_t);
static int (*fn_pcm_recover)(snd_pcm_t *, int, int);
static int (*fn_pcm_prepare)(snd_pcm_t *);
static int (*fn_pcm_drop)(snd_pcm_t *);
static int (*fn_pcm_drain)(snd_pcm_t *);
static const char *(*fn_strerror)(int);

static int (*fn_mixer_open)(snd_mixer_t **, int);
static int (*fn_mixer_close)(snd_mixer_t *);
static int (*fn_mixer_attach)(snd_mixer_t *, const char *);
static int (*fn_mixer_selem_register)(snd_mixer_t *, void *, void **);
static int (*fn_mixer_load)(snd_mixer_t *);
static size_t (*fn_selem_id_sizeof)(void);
static void (*fn_selem_id_set_index)(snd_mixer_selem_id_t *, unsigned int);
static void (*fn_selem_id_set_name)(snd_mixer_selem_id_t *, const char *);
static snd_mixer_elem_t *(*fn_mixer_find_selem)(snd_mixer_t *, const snd_mixer_selem_id_t *);
static int (*fn_selem_get_range)(snd_mixer_elem_t *, long *, long *);
static int (*fn_selem_get_vol)(snd_mixer_elem_t *, int, long *);

#define LOAD_SYM(var, name)                                     \
    do {                                                        \
        *(void **)&var = dlsym(lib, name);                      \
        if (!var) {                                             \
            fprintf(stderr, "alsasink: missing %s\n", name);    \
            return 0;                                           \
        }                                                       \
    } while (0)

static int load_alsa(void) {
    void *lib = dlopen("/usr/lib/libasound.so.2", RTLD_NOW);
    if (!lib) lib = dlopen("libasound.so.2", RTLD_NOW);
    if (!lib) {
        fprintf(stderr, "alsasink: dlopen libasound: %s\n", dlerror());
        return 0;
    }
    LOAD_SYM(fn_pcm_open, "snd_pcm_open");
    LOAD_SYM(fn_pcm_close, "snd_pcm_close");
    LOAD_SYM(fn_pcm_set_params, "snd_pcm_set_params");
    LOAD_SYM(fn_pcm_writei, "snd_pcm_writei");
    LOAD_SYM(fn_pcm_recover, "snd_pcm_recover");
    LOAD_SYM(fn_pcm_prepare, "snd_pcm_prepare");
    LOAD_SYM(fn_pcm_drop, "snd_pcm_drop");
    LOAD_SYM(fn_pcm_drain, "snd_pcm_drain");
    LOAD_SYM(fn_strerror, "snd_strerror");

    /* Микшерные символы — опциональны (если нет, VOL вернёт -1) */
    *(void **)&fn_mixer_open = dlsym(lib, "snd_mixer_open");
    *(void **)&fn_mixer_close = dlsym(lib, "snd_mixer_close");
    *(void **)&fn_mixer_attach = dlsym(lib, "snd_mixer_attach");
    *(void **)&fn_mixer_selem_register = dlsym(lib, "snd_mixer_selem_register");
    *(void **)&fn_mixer_load = dlsym(lib, "snd_mixer_load");
    *(void **)&fn_selem_id_sizeof = dlsym(lib, "snd_mixer_selem_id_sizeof");
    *(void **)&fn_selem_id_set_index = dlsym(lib, "snd_mixer_selem_id_set_index");
    *(void **)&fn_selem_id_set_name = dlsym(lib, "snd_mixer_selem_id_set_name");
    *(void **)&fn_mixer_find_selem = dlsym(lib, "snd_mixer_find_selem");
    *(void **)&fn_selem_get_range = dlsym(lib, "snd_mixer_selem_get_playback_volume_range");
    *(void **)&fn_selem_get_vol = dlsym(lib, "snd_mixer_selem_get_playback_volume");
    return 1;
}

static int read_exact(int fd, void *buf, size_t n) {
    uint8_t *p = (uint8_t *)buf;
    size_t got = 0;
    while (got < n) {
        ssize_t r = read(fd, p + got, n - got);
        if (r == 0) return 0;
        if (r < 0) {
            if (errno == EINTR) continue;
            return -1;
        }
        got += (size_t)r;
    }
    return 1;
}

static int read_master_percent_card(const char *card) {
    if (!fn_mixer_open || !fn_mixer_close || !fn_mixer_attach || !fn_mixer_selem_register ||
        !fn_mixer_load || !fn_selem_id_sizeof || !fn_selem_id_set_index || !fn_selem_id_set_name ||
        !fn_mixer_find_selem || !fn_selem_get_range || !fn_selem_get_vol) {
        return -1;
    }
    snd_mixer_t *h = NULL;
    if (fn_mixer_open(&h, 0) < 0 || !h) return -1;
    int pct = -1;
    if (fn_mixer_attach(h, card) >= 0 &&
        fn_mixer_selem_register(h, NULL, NULL) >= 0 &&
        fn_mixer_load(h) >= 0) {
        size_t sz = fn_selem_id_sizeof();
        if (sz > 0 && sz <= 4096) {
            snd_mixer_selem_id_t *sid = calloc(1, sz);
            if (sid) {
                fn_selem_id_set_index(sid, 0);
                fn_selem_id_set_name(sid, "Master");
                snd_mixer_elem_t *elem = fn_mixer_find_selem(h, sid);
                if (elem) {
                    long lo = 0, hi = 0, cur = 0;
                    if (fn_selem_get_range(elem, &lo, &hi) >= 0 && hi > lo &&
                        fn_selem_get_vol(elem, SND_MIXER_SCHN_FRONT_LEFT, &cur) >= 0) {
                        long span = hi - lo;
                        long val = cur - lo;
                        if (val < 0) val = 0;
                        if (val > span) val = span;
                        pct = (int)((val * 100L + span / 2L) / span);
                    }
                }
                free(sid);
            }
        }
    }
    fn_mixer_close(h);
    return pct;
}

static int read_master_percent(void) {
    int p = read_master_percent_card("hw:0");
    if (p < 0) p = read_master_percent_card("default");
    return p;
}

static void close_pcm(snd_pcm_t **pcm, int drain) {
    if (!*pcm) return;
    if (drain) {
        fn_pcm_drain(*pcm);
    } else {
        fn_pcm_drop(*pcm);
    }
    fn_pcm_close(*pcm);
    *pcm = NULL;
}

static int write_pcm_all(snd_pcm_t *pcm, const uint8_t *data, size_t bytes) {
    if (!pcm || bytes < 2) return 0;
    size_t total_frames = bytes / 2;
    size_t offset = 0;
    while (offset < total_frames) {
        snd_pcm_sframes_t written = fn_pcm_writei(pcm, data + offset * 2, total_frames - offset);
        if (written < 0) {
            int rec = fn_pcm_recover(pcm, (int)written, 1);
            if (rec < 0) {
                if (fn_pcm_prepare(pcm) < 0) {
                    return rec;
                }
            }
            continue;
        }
        if (written == 0) break;
        offset += (size_t)written;
    }
    return 0;
}

int main(int argc, char **argv) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    if (!load_alsa()) return 2;

    if (argc > 1 && strcmp(argv[1], "--vol") == 0) {
        printf("%d\n", read_master_percent());
        return 0;
    }

    fprintf(stderr, "alsasink: ready\n");
    snd_pcm_t *pcm = NULL;
    uint8_t *buf = NULL;
    size_t buf_cap = 0;

    uint8_t hdr[5];
    for (;;) {
        int r = read_exact(STDIN_FILENO, hdr, sizeof(hdr));
        if (r <= 0) break;
        uint8_t op = hdr[0];
        uint32_t arg = (uint32_t)hdr[1] |
                       ((uint32_t)hdr[2] << 8) |
                       ((uint32_t)hdr[3] << 16) |
                       ((uint32_t)hdr[4] << 24);

        if (op == 'O') {
            close_pcm(&pcm, 0);
            unsigned int rate = (arg >= 8000 && arg <= 96000) ? arg : 48000;
            int rc = fn_pcm_open(&pcm, "default", SND_PCM_STREAM_PLAYBACK, 0);
            if (rc < 0) {
                pcm = NULL;
                printf("ERR open:%s\n", fn_strerror(rc));
                continue;
            }
            rc = fn_pcm_set_params(
                pcm,
                SND_PCM_FORMAT_S16_LE,
                SND_PCM_ACCESS_RW_INTERLEAVED,
                1,
                rate,
                1,
                100000 /* 100 ms latency */
            );
            if (rc < 0) {
                fn_pcm_close(pcm);
                pcm = NULL;
                printf("ERR params:%s\n", fn_strerror(rc));
                continue;
            }
            printf("OK\n");
        } else if (op == 'W') {
            if (arg == 0) continue;
            if (arg > 2 * 1024 * 1024) {
                fprintf(stderr, "alsasink: chunk too big (%u)\n", arg);
                break;
            }
            if (arg > buf_cap) {
                uint8_t *nb = realloc(buf, arg);
                if (!nb) break;
                buf = nb;
                buf_cap = arg;
            }
            if (read_exact(STDIN_FILENO, buf, arg) <= 0) break;
            if (pcm) {
                int wrc = write_pcm_all(pcm, buf, arg);
                if (wrc < 0) {
                    fprintf(stderr, "alsasink: write error: %s\n", fn_strerror(wrc));
                    close_pcm(&pcm, 0);
                }
            }
        } else if (op == 'X') {
            close_pcm(&pcm, 0);
            printf("OK\n");
        } else if (op == 'D') {
            close_pcm(&pcm, 1);
            printf("OK\n");
        } else if (op == 'V') {
            printf("VOL %d\n", read_master_percent());
        } else if (op == 'Q') {
            close_pcm(&pcm, 0);
            break;
        } else {
            fprintf(stderr, "alsasink: unknown op 0x%02x\n", op);
            break;
        }
    }

    close_pcm(&pcm, 0);
    free(buf);
    return 0;
}
