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
    measured_bitrates = []
    gcc_estimates = []
    rtts = []
    timestamps = []
    cwnds = []
    avg_losses = []

    overuse_detector_thresholds = []
    overuse_detector_estimates = [] 
    usages = []

    delay_estimates = []
    loss_estimates = []
    # inter delay variations
    idvs = []

    epoch = dt.datetime(2000,1, 1)

    with open("gcc_output.log") as file:
        for i, line in enumerate(file):
            data = json.loads(line)
            timestamp = data["timestamp"]
            # Only render every 5th data point
            if i % 10 != 0:
                continue

            fields = data["fields"]

            # Timestamp
            ts = dt.datetime.strptime(timestamp, '%Y-%m-%dT%H:%M:%S.%fZ')
            if i == 0:
                epoch = ts
                timestamps.append(0)
            else:
                timestamps.append((ts-epoch).total_seconds())

            # cwnd
            cwnds.append(fields["window"])

            # Loss
            avg_losses.append(float(fields["average_loss"]))

            # Measured bitrate
            try:
                measured_bitrates.append(kb2num(fields["measurement"]))
            except:
                measured_bitrates.append(0)

            # GCC Estimate
            gcc_estimates.append(kb2num(fields["estimate"]))

            # RTT
            rtts.append(float(fields["rtt"]))

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


    PLOT_ROWS=4
    PLOT_COLS=1

    # Capacity and bitrate plot
    ax = plt.subplot(PLOT_ROWS, PLOT_COLS, 1)
    ax.set_ylabel("kbps")
    measurement_handle, = ax.plot(timestamps, measured_bitrates, label='measured bitrate')
    estimate_handle, = ax.plot(timestamps, gcc_estimates, label='gcc estimates')
    cap_handle = plt.axhline(y=1000, label='capacity', color='black', linestyle='--')

    # Congestion window
    cwnd_ax = ax.twinx()
    cwnd_handle, = cwnd_ax.plot(timestamps, cwnds, color="red", label="window size")
    cwnd_ax.set_ylabel("bytes")

    ax.legend(handles=[measurement_handle, estimate_handle, cwnd_handle, cap_handle], loc="lower left")

    # RTT Plot
    ax = plt.subplot(PLOT_ROWS, PLOT_COLS, 2)
    ax.plot(timestamps, rtts, 'g-', label='rtt')
    ax.set_ylabel("ms")
    ax.legend(loc="best")

    # Average Loss
    ax = plt.subplot(PLOT_ROWS, PLOT_COLS, 3)
    ax.plot(timestamps, avg_losses, label="Average Loss")
    ax.set_ylabel("%")
    ax.legend(loc="best")

    # Adaptive Threshold Plot
    ax = plt.subplot(PLOT_ROWS, PLOT_COLS, 4)
    ax.plot(timestamps, overuse_detector_thresholds, label='γ(ti)')
    ax.plot(timestamps, overuse_detector_estimates, label='m(ti)')
    ax.plot(timestamps, list(map(lambda x: -x, overuse_detector_thresholds)), label='-γ(ti)')
    ax.set_title(label='Adaptive Threshold Internals')
    for i, usage in enumerate(usages):
        if i > 0:
            label = 'Normal'
            color = 'honeydew'
            if usage == OVER:
                color = 'lightpink'
                label = 'Overuse'
            elif usage == UNDER:
                color  = 'lightsteelblue'
                label = 'Underuse'
            ax.axvspan(timestamps[i-1], timestamps[i], facecolor=color)

    # Overuse
    ax.legend(loc="best")
    ax.set_xlabel("t (seconds)")

    """
    ax = plt.subplot(PLOT_ROWS, PLOT_COLS, 5)
    ax.plot(time, loss_estimates, label='loss estimates')
    ax.plot(time, delay_estimates, label='delay estimates')
    ax.set_ylabel("kbps")
    ax.legend(loc="upper right")

    ax = plt.subplot(PLOT_ROWS, PLOT_COLS, 6)
    ax.plot(time, idvs, label='inter-group delay variation')
    ax.set_ylabel("m(ti)")
    ax.legend(loc="best")
    """

    plt.show()
    

run()

