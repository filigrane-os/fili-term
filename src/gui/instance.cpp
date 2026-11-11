#include "instance.h"

#include <QDir>
#include <QLocalSocket>
#include <QQmlComponent>
#include <QQmlContext>
#include <QQmlEngine>
#include <cstdio>
#include <unistd.h>

namespace {
QString uiSocket() {
    QByteArray dir = qgetenv("XDG_RUNTIME_DIR");
    if (dir.isEmpty()) dir = QByteArray("/tmp/fili-term-") + QByteArray::number(getuid());
    return QString::fromLocal8Bit(dir) + QStringLiteral("/fili-term/ui.sock");
}
}  // namespace

Instance::Instance(QQmlEngine* engine, QObject* parent) : QObject(parent), m_engine(engine) {
    m_window = new QQmlComponent(engine, QUrl(QStringLiteral("qrc:/qt/qml/Fili/Term/qml/TermWindow.qml")), this);
    if (m_window->isError()) fprintf(stderr, "fili-term: %s\n", qPrintable(m_window->errorString()));
    engine->rootContext()->setContextProperty(QStringLiteral("Instance"), this);
    QString path = uiSocket();
    QLocalServer::removeServer(path);
    m_server.setSocketOptions(QLocalServer::UserAccessOption);
    if (!m_server.listen(path)) fprintf(stderr, "fili-term: %s: %s\n", qPrintable(path), qPrintable(m_server.errorString()));
    connect(&m_server, &QLocalServer::newConnection, this, [this] {
        while (QLocalSocket* s = m_server.nextPendingConnection()) {
            connect(s, &QLocalSocket::readyRead, s, [this, s] {
                while (s->canReadLine()) handle(QString::fromUtf8(s->readLine()).chopped(1));
            });
            connect(s, &QLocalSocket::disconnected, s, &QObject::deleteLater);
        }
    });
}

void Instance::handle(const QString& request) {
    QStringList parts = request.split(QLatin1Char('\t'));
    if (parts.size() < 3 || parts[0] != QLatin1String("open")) return;
    open(parts[1].toInt(), parts[2], parts.mid(3));
}

void Instance::newWindow(const QString& cwd) { open(0, cwd.isEmpty() ? QDir::homePath() : cwd, {}); }

void Instance::open(int attach, const QString& cwd, const QStringList& command) {
    QVariantMap props{{QStringLiteral("attachId"), attach}, {QStringLiteral("cwd"), cwd},
                      {QStringLiteral("command"), command}};
    QObject* w = m_window->createWithInitialProperties(props);
    if (!w) {
        fprintf(stderr, "fili-term: %s\n", qPrintable(m_window->errorString()));
        return;
    }
    // Kept alive by the instance; the window destroys itself when closed (TermWindow.qml)
    w->setParent(this);
}

extern "C" void fili_term_start(void* engine, const char* request) {
    auto* instance = new Instance(static_cast<QQmlEngine*>(engine));
    instance->handle(QString::fromUtf8(request));
}
