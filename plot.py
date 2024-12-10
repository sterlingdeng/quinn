import matplotlib.pyplot as plt
import numpy as np
import json
import datetime as dt

def kb2num(v):
    v = v[:len(v)-2]
    v = float(v)
    return v

NORMAL=0
OVER=1
UNDER=-1

def run():
    measurements = []
    estimates = []
    rtt = []
    time = []
    cwnds = []
    avg_losses = []

    overuse_detector_thresholds = []
    overuse_detector_estimates = [] 
    usages = []

    delay_estimates = []
    loss_estimates = []


    # inter delay variations
    idvs = []

    oldtime = dt.datetime(2000,1, 1)

    with open("metrics.log") as file:
        for i, line in enumerate(file):

            data = json.loads(line)
            timestamp = data["timestamp"]
            # Only render every 10th data point
            if i % 10 != 0:
                continue

            fields = data["fields"]

            # TIME
            ts = dt.datetime.strptime(timestamp, '%Y-%m-%dT%H:%M:%S.%fZ')
            if i == 0:
                oldtime = ts
                time.append(0)
            else:
                time.append((ts-oldtime).total_seconds())

            # cwnd
            cwnds.append(fields["window"])

            # Loss
            avg_losses.append(float(fields["average_loss"]))

            # Effective bitrate
            try:
                measurements.append(kb2num(fields["measurement"]))
            except:
                measurements.append(0)

            # GCC Estimate
            estimates.append(kb2num(fields["estimate"]))
            # RTT
            rtt.append(float(fields["rtt"]))

            # Overuse detector
            overuse_detector_estimates.append(int(fields["overuse_detector_estimate"]))
            overuse_detector_thresholds.append(int(fields["overuse_detector_threshold"]))

            usage = NORMAL
            if fields["usage"] == "over": 
                usage = OVER
            elif fields["usage"] == "under":
                usage = UNDER
            usages.append(usage)

            delay_estimates.append(float(fields["delay_ctrl_bitrate"])/1000)
            loss_estimates.append(float(fields["loss_ctrl_bitrate"])/1000)

            idvs.append(int(fields["idv"]))


    PLOT_ROWS=6
    PLOT_COLS=1

    # Capacity and bitrate plot
    ax = plt.subplot(PLOT_ROWS, PLOT_COLS, 1)
    ax.set_ylabel("kbps")
    measurement_handle, = ax.plot(time, measurements, label='measurements')
    estimate_handle, = ax.plot(time, estimates, label='gcc estimates')
    cap_handle = plt.axhline(y=1000, label='capacity', color='black', linestyle='--')

    cwnd_ax = ax.twinx()
    cwnd_handle, = cwnd_ax.plot(time, cwnds, color="red", label="window size")
    cwnd_ax.set_ylabel("bytes")
    #cwnd_ax.set_ylim(2_000, 500_000)

    ax.legend(handles=[measurement_handle, estimate_handle, cwnd_handle, cap_handle], loc="upper left")
    ax.plot()


    # RTT Plot
    ax = plt.subplot(PLOT_ROWS, PLOT_COLS, 2)
    rtt_handle, = ax.plot(time, rtt, 'g-', label='rtt')
    ax.set_ylabel("ms")
    ax.legend(loc="best")

    ax = plt.subplot(PLOT_ROWS, PLOT_COLS, 3)
    ax.plot(time, avg_losses, label="Average Loss")
    ax.set_ylabel("%")
    ax.legend(loc="best")
    ax.plot()

    # Adaptive Threshold Plot
    ax = plt.subplot(PLOT_ROWS, PLOT_COLS, 4)
    ax.plot(time, overuse_detector_thresholds, label='γ(ti)')
    ax.plot(time, overuse_detector_estimates, label='m(ti)')
    ax.plot(time, list(map(lambda x: -x, overuse_detector_thresholds)), label='-γ(ti)')
    ax.legend(loc="best")
    ax.set_title(label='Adaptive Threshold Internals')
    for i, usage in enumerate(usages):
        if i > 0:
            color = 'honeydew'
            if usage == OVER:
                color = 'lightpink'
            elif usage == UNDER:
                color  = 'lightsteelblue'
            ax.axvspan(time[i-1], time[i], facecolor=color)

    # Overuse
    ax.set_xlabel("t (seconds)")

    ax = plt.subplot(PLOT_ROWS, PLOT_COLS, 5)
    ax.plot(time, loss_estimates, label='loss estimates')
    ax.plot(time, delay_estimates, label='delay estimates')
    ax.set_ylabel("kbps")
    ax.legend(loc="upper right")

    ax = plt.subplot(PLOT_ROWS, PLOT_COLS, 6)
    ax.plot(time, idvs, label='inter-group delay variation')
    ax.set_ylabel("m(ti)")
    ax.legend(loc="best")

    plt.show()
    

run()

